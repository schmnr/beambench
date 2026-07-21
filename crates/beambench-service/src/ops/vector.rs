use std::collections::{HashMap, HashSet};
use std::io::Cursor;

use beambench_common::geometry::Transform2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use beambench_common::{Bounds, Point2D};
use beambench_core::object::GuideAxis;
use beambench_core::vector::boolean::{
    path_exclude, path_intersection, path_subtract, path_union, weld_shapes,
};
use beambench_core::vector::convert::{object_to_world_vecpath, object_to_world_vecpath_resolved};
use beambench_core::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use beambench_core::vector::node_edit::{self, EditablePath, HandleType, NodeId, NodeType};
use beambench_core::vector::normalize::{NormalizedVector, normalize_object};
use beambench_core::vector::path_ops as path_ops_core;
use beambench_core::vector::transform::bake_transform;
use beambench_core::vector::trim as trim_core;
use beambench_core::{
    Asset, AssetId, AssetMediaType, ObjectData, ObjectId, Project, ProjectObject,
};
use image::RgbaImage;
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

use crate::context::ServiceContext;
use crate::error::{ServiceError, ServiceResult};
use crate::events;
use crate::ops::planning;

#[derive(Debug, Clone)]
pub struct ConvertToPathInput {
    pub object_id: ObjectId,
}

#[derive(Debug, Clone)]
pub struct BooleanOpInput {
    pub object_id_a: ObjectId,
    pub object_id_b: ObjectId,
}

#[derive(Debug, Clone)]
pub struct BooleanWeldInput {
    pub object_ids: Vec<ObjectId>,
}

#[derive(Debug, Clone)]
pub struct GroupObjectsInput {
    pub object_ids: Vec<ObjectId>,
}

#[derive(Debug, Clone)]
pub struct AutoGroupObjectsInput {
    pub object_ids: Vec<ObjectId>,
}

#[derive(Debug, Clone)]
pub struct UpdateNodeInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
    pub command_idx: usize,
    pub x: f64,
    pub y: f64,
    pub handle_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BatchNodeUpdate {
    pub node_id: NodeId,
    pub x: f64,
    pub y: f64,
    pub handle_type: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateNodesBatchInput {
    pub object_id: ObjectId,
    pub updates: Vec<BatchNodeUpdate>,
}

#[derive(Debug, Clone)]
pub struct DeleteNodeInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
    pub command_idx: usize,
}

#[derive(Debug, Clone)]
pub struct DeleteNodesInput {
    pub object_id: ObjectId,
    pub node_ids: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct SetNodeTypeInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
    pub command_idx: usize,
    pub node_type: String,
}

#[derive(Debug, Clone)]
pub struct InsertNodeInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
    pub command_idx: usize,
    pub t: f64,
}

#[derive(Debug, Clone)]
pub struct SegmentOpInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
    pub command_idx: usize,
}

#[derive(Debug, Clone)]
pub struct ExtendEndpointInput {
    pub object_id: ObjectId,
    pub node_id: NodeId,
}

#[derive(Debug, Clone)]
pub struct JoinSubpathsInput {
    pub object_id: ObjectId,
    pub src_node_id: NodeId,
    pub dst_node_id: NodeId,
}

#[derive(Debug, Clone)]
pub struct SubpathOpInput {
    pub object_id: ObjectId,
    pub subpath_idx: usize,
}

#[derive(Debug, Clone)]
pub struct ScalePathToBoundsInput {
    pub object_id: ObjectId,
    pub new_min_x: f64,
    pub new_min_y: f64,
    pub new_max_x: f64,
    pub new_max_y: f64,
}

#[derive(Debug, Clone)]
pub struct MeshDeformSelectionInput {
    pub object_ids: Vec<ObjectId>,
    pub source_bounds: Bounds,
    pub handles: Vec<Point2D>,
    pub grid_size: usize,
    pub perspective: bool,
}

#[derive(Debug, Clone)]
pub struct NormalizeForPlannerInput {
    pub object_ids: Vec<ObjectId>,
}

fn lock_err(name: &str, e: impl std::fmt::Display) -> ServiceError {
    ServiceError::internal(format!("Failed to lock {name}: {e}"))
}

fn require_vector_path(obj: &ProjectObject) -> ServiceResult<VecPath> {
    match &obj.data {
        ObjectData::VectorPath { path_data, .. } => Ok(VecPath::parse_svg_d(path_data)),
        ObjectData::VirtualClone { .. } => Err(ServiceError::invalid_input(
            "VirtualClone must be resolved before accessing geometry",
        )),
        _ => Err(ServiceError::invalid_input("Object is not a VectorPath")),
    }
}

fn reroute_vector_result_layer(
    project: &mut beambench_core::Project,
    requested_layer: beambench_core::LayerId,
) -> ServiceResult<beambench_core::LayerId> {
    crate::validation::resolve_layer_for_object(
        project,
        requested_layer,
        crate::validation::RoutingTarget::NeedsNonImage,
    )
    .map(|(layer_id, _)| layer_id)
}

fn reroute_image_result_layer(
    project: &mut beambench_core::Project,
    requested_layer: beambench_core::LayerId,
) -> ServiceResult<beambench_core::LayerId> {
    crate::validation::resolve_layer_for_object(
        project,
        requested_layer,
        crate::validation::RoutingTarget::NeedsImage,
    )
    .map(|(layer_id, _)| layer_id)
}

fn get_vector_path_meta(obj: &ProjectObject) -> ServiceResult<Option<GuideAxis>> {
    match &obj.data {
        ObjectData::VectorPath {
            ruler_guide_axis, ..
        } => Ok(*ruler_guide_axis),
        ObjectData::VirtualClone { .. } => Err(ServiceError::invalid_input(
            "VirtualClone must be resolved before accessing geometry",
        )),
        _ => Err(ServiceError::invalid_input("Object is not a VectorPath")),
    }
}

fn write_vec_path_to_object(
    obj: &mut ProjectObject,
    vec_path: &VecPath,
    ruler_guide_axis: Option<GuideAxis>,
) {
    let new_bounds = vec_path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);
    obj.data = ObjectData::VectorPath {
        path_data: vec_path.to_svg_d(),
        closed,
        ruler_guide_axis,
    };
    obj.bounds = new_bounds;
    obj.tabs.clear();
}

fn command_has_editable_node(command: &PathCommand) -> bool {
    !matches!(command, PathCommand::Close)
}

fn command_draws_segment(command: &PathCommand) -> bool {
    matches!(
        command,
        PathCommand::LineTo { .. } | PathCommand::QuadTo { .. } | PathCommand::CubicTo { .. }
    )
}

fn subpath_has_drawable_segment(subpath: &SubPath) -> bool {
    subpath.commands.iter().any(command_draws_segment)
}

fn prune_degenerate_subpaths(vec_path: &mut VecPath) {
    vec_path.subpaths.retain(subpath_has_drawable_segment);
}

#[derive(Debug, Clone)]
struct MeshDeformMapper {
    source_bounds: Bounds,
    handles: Vec<Point2D>,
    grid_size: usize,
    perspective: bool,
}

impl MeshDeformMapper {
    fn new(input: &MeshDeformSelectionInput) -> ServiceResult<Self> {
        if !(2..=8).contains(&input.grid_size) {
            return Err(ServiceError::invalid_input(
                "Deform grid size must be between 2 and 8",
            ));
        }
        let expected = input.grid_size * input.grid_size;
        if input.handles.len() != expected {
            return Err(ServiceError::invalid_input(format!(
                "Expected {expected} deform handles, got {}",
                input.handles.len()
            )));
        }
        if input.source_bounds.width().abs() <= 1e-9 || input.source_bounds.height().abs() <= 1e-9 {
            return Err(ServiceError::invalid_input(
                "Cannot deform a zero-size selection",
            ));
        }

        Ok(Self {
            source_bounds: input.source_bounds,
            handles: input.handles.clone(),
            grid_size: input.grid_size,
            perspective: input.perspective && input.grid_size == 2,
        })
    }

    fn map_point(&self, point: Point2D) -> Point2D {
        let u = ((point.x - self.source_bounds.min.x) / self.source_bounds.width()).clamp(0.0, 1.0);
        let v =
            ((point.y - self.source_bounds.min.y) / self.source_bounds.height()).clamp(0.0, 1.0);

        if self.perspective {
            return self.map_perspective(u, v);
        }

        self.map_grid(u, v)
    }

    fn map_perspective(&self, u: f64, v: f64) -> Point2D {
        let p00 = self.handles[0];
        let p10 = self.handles[1];
        let p01 = self.handles[2];
        let p11 = self.handles[3];

        let dx1 = p10.x - p11.x;
        let dx2 = p01.x - p11.x;
        let dx3 = p00.x - p10.x + p11.x - p01.x;
        let dy1 = p10.y - p11.y;
        let dy2 = p01.y - p11.y;
        let dy3 = p00.y - p10.y + p11.y - p01.y;
        let det = dx1 * dy2 - dx2 * dy1;

        if det.abs() <= 1e-12 {
            return self.map_grid(u, v);
        }

        let g = (dx3 * dy2 - dx2 * dy3) / det;
        let h = (dx1 * dy3 - dx3 * dy1) / det;
        let a = p10.x - p00.x + g * p10.x;
        let b = p01.x - p00.x + h * p01.x;
        let c = p00.x;
        let d = p10.y - p00.y + g * p10.y;
        let e = p01.y - p00.y + h * p01.y;
        let f = p00.y;
        let denom = g * u + h * v + 1.0;

        if denom.abs() <= 1e-12 {
            return self.map_grid(u, v);
        }

        Point2D::new((a * u + b * v + c) / denom, (d * u + e * v + f) / denom)
    }

    fn map_grid(&self, u: f64, v: f64) -> Point2D {
        let last_cell = self.grid_size - 2;
        let scaled_u = u * (self.grid_size - 1) as f64;
        let scaled_v = v * (self.grid_size - 1) as f64;
        let col = (scaled_u.floor() as usize).min(last_cell);
        let row = (scaled_v.floor() as usize).min(last_cell);
        let fu = scaled_u - col as f64;
        let fv = scaled_v - row as f64;

        let p00 = self.handle(col, row);
        let p10 = self.handle(col + 1, row);
        let p01 = self.handle(col, row + 1);
        let p11 = self.handle(col + 1, row + 1);

        let top = p00.lerp(&p10, fu);
        let bottom = p01.lerp(&p11, fu);
        top.lerp(&bottom, fv)
    }

    fn handle(&self, col: usize, row: usize) -> Point2D {
        self.handles[row * self.grid_size + col]
    }

    fn segment_steps(&self, from: Point2D, to: Point2D) -> usize {
        if self.perspective {
            return 1;
        }
        let cell = (self
            .source_bounds
            .width()
            .abs()
            .max(self.source_bounds.height().abs())
            / (self.grid_size - 1) as f64)
            .max(1.0);
        ((from.distance_to(&to) / (cell * 0.5)).ceil() as usize).clamp(1, 96)
    }
}

fn quad_point(p0: Point2D, c: Point2D, p1: Point2D, t: f64) -> Point2D {
    let mt = 1.0 - t;
    Point2D::new(
        mt * mt * p0.x + 2.0 * mt * t * c.x + t * t * p1.x,
        mt * mt * p0.y + 2.0 * mt * t * c.y + t * t * p1.y,
    )
}

fn cubic_point(p0: Point2D, c1: Point2D, c2: Point2D, p1: Point2D, t: f64) -> Point2D {
    let mt = 1.0 - t;
    Point2D::new(
        mt * mt * mt * p0.x + 3.0 * mt * mt * t * c1.x + 3.0 * mt * t * t * c2.x + t * t * t * p1.x,
        mt * mt * mt * p0.y + 3.0 * mt * mt * t * c1.y + 3.0 * mt * t * t * c2.y + t * t * t * p1.y,
    )
}

fn push_deformed_line(
    commands: &mut Vec<PathCommand>,
    mapper: &MeshDeformMapper,
    from: Point2D,
    to: Point2D,
) {
    let steps = mapper.segment_steps(from, to);
    for step in 1..=steps {
        let t = step as f64 / steps as f64;
        let src = from.lerp(&to, t);
        let dst = mapper.map_point(src);
        commands.push(PathCommand::LineTo { x: dst.x, y: dst.y });
    }
}

fn deform_vec_path(path: &VecPath, mapper: &MeshDeformMapper) -> VecPath {
    let mut result = VecPath::new();

    for subpath in &path.subpaths {
        let mut next = SubPath::new();
        next.closed = subpath.closed;
        let mut current: Option<Point2D> = None;
        let mut start: Option<Point2D> = None;

        for command in &subpath.commands {
            match *command {
                PathCommand::MoveTo { x, y } => {
                    let src = Point2D::new(x, y);
                    let dst = mapper.map_point(src);
                    next.commands
                        .push(PathCommand::MoveTo { x: dst.x, y: dst.y });
                    current = Some(src);
                    start = Some(src);
                }
                PathCommand::LineTo { x, y } => {
                    let to = Point2D::new(x, y);
                    if let Some(from) = current {
                        push_deformed_line(&mut next.commands, mapper, from, to);
                    } else {
                        let dst = mapper.map_point(to);
                        next.commands
                            .push(PathCommand::MoveTo { x: dst.x, y: dst.y });
                        start = Some(to);
                    }
                    current = Some(to);
                }
                PathCommand::QuadTo { cx, cy, x, y } => {
                    let Some(from) = current else {
                        continue;
                    };
                    let control = Point2D::new(cx, cy);
                    let to = Point2D::new(x, y);
                    for step in 1..=24 {
                        let t = step as f64 / 24.0;
                        let dst = mapper.map_point(quad_point(from, control, to, t));
                        next.commands
                            .push(PathCommand::LineTo { x: dst.x, y: dst.y });
                    }
                    current = Some(to);
                }
                PathCommand::CubicTo {
                    c1x,
                    c1y,
                    c2x,
                    c2y,
                    x,
                    y,
                } => {
                    let Some(from) = current else {
                        continue;
                    };
                    let c1 = Point2D::new(c1x, c1y);
                    let c2 = Point2D::new(c2x, c2y);
                    let to = Point2D::new(x, y);
                    for step in 1..=32 {
                        let t = step as f64 / 32.0;
                        let dst = mapper.map_point(cubic_point(from, c1, c2, to, t));
                        next.commands
                            .push(PathCommand::LineTo { x: dst.x, y: dst.y });
                    }
                    current = Some(to);
                }
                PathCommand::Close => {
                    if let (Some(from), Some(to)) = (current, start) {
                        push_deformed_line(&mut next.commands, mapper, from, to);
                        current = Some(to);
                    }
                    next.commands.push(PathCommand::Close);
                    next.closed = true;
                }
            }
        }

        if !next.commands.is_empty() {
            result.subpaths.push(next);
        }
    }

    result
}

fn transformed_raster_point(object: &ProjectObject, point: Point2D) -> Point2D {
    if object.transform.is_identity() {
        point
    } else {
        let center = Point2D::new(
            (object.bounds.min.x + object.bounds.max.x) / 2.0,
            (object.bounds.min.y + object.bounds.max.y) / 2.0,
        );
        object.transform.apply_around_center(&point, &center)
    }
}

fn deformed_raster_bounds(object: &ProjectObject, mapper: &MeshDeformMapper) -> Bounds {
    let corners = [
        object.bounds.min,
        Point2D::new(object.bounds.max.x, object.bounds.min.y),
        object.bounds.max,
        Point2D::new(object.bounds.min.x, object.bounds.max.y),
    ];
    let mapped = corners.map(|point| mapper.map_point(transformed_raster_point(object, point)));
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for point in mapped {
        min_x = min_x.min(point.x);
        min_y = min_y.min(point.y);
        max_x = max_x.max(point.x);
        max_y = max_y.max(point.y);
    }
    if (max_x - min_x).abs() <= 1e-9 {
        max_x = min_x + 1.0;
    }
    if (max_y - min_y).abs() <= 1e-9 {
        max_y = min_y + 1.0;
    }
    Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y))
}

fn write_raster_pixel(dest: &mut RgbaImage, x: i32, y: i32, pixel: image::Rgba<u8>) {
    if x < 0 || y < 0 {
        return;
    }
    let (width, height) = dest.dimensions();
    let (x, y) = (x as u32, y as u32);
    if x >= width || y >= height {
        return;
    }
    if pixel[3] >= dest.get_pixel(x, y)[3] {
        dest.put_pixel(x, y, pixel);
    }
}

fn encode_raster_png(image: RgbaImage) -> ServiceResult<Vec<u8>> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|e| ServiceError::internal(format!("Failed to encode deformed raster: {e}")))?;
    Ok(bytes.into_inner())
}

fn deform_raster_object(
    project: &mut Project,
    object_id: ObjectId,
    mapper: &MeshDeformMapper,
) -> ServiceResult<()> {
    let object = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?
        .clone();
    let (asset_key, adjustments) = match &object.data {
        ObjectData::RasterImage {
            asset_key,
            adjustments,
            ..
        } => (asset_key.clone(), adjustments.clone()),
        _ => {
            return Err(ServiceError::invalid_input(
                "Warp and Deform require raster image data",
            ));
        }
    };
    let asset_id = AssetId::from_uuid(
        Uuid::parse_str(&asset_key)
            .map_err(|_| ServiceError::invalid_input("Invalid image asset reference"))?,
    );
    let bytes = project
        .asset_data
        .get(&asset_id)
        .cloned()
        .ok_or_else(|| ServiceError::not_found("Image asset data not found"))?;
    let source = image::load_from_memory(&bytes)
        .map_err(|e| ServiceError::invalid_input(format!("Failed to decode image: {e}")))?
        .to_rgba8();
    let (width_px, height_px) = source.dimensions();
    if width_px == 0 || height_px == 0 {
        return Err(ServiceError::invalid_input("Image has no pixels"));
    }

    let dest_bounds = deformed_raster_bounds(&object, mapper);
    let mut dest = RgbaImage::new(width_px, height_px);
    let x_step = object.bounds.width() / width_px as f64;
    let y_step = object.bounds.height() / height_px as f64;
    let dest_w = dest_bounds.width().abs().max(1e-9);
    let dest_h = dest_bounds.height().abs().max(1e-9);
    for y in 0..height_px {
        for x in 0..width_px {
            let pixel = *source.get_pixel(x, y);
            if pixel[3] == 0 {
                continue;
            }
            let local = Point2D::new(
                object.bounds.min.x + (x as f64 + 0.5) * x_step,
                object.bounds.min.y + (y as f64 + 0.5) * y_step,
            );
            let world = transformed_raster_point(&object, local);
            let mapped = mapper.map_point(world);
            let dest_x = ((mapped.x - dest_bounds.min.x) / dest_w * width_px as f64).round() as i32;
            let dest_y =
                ((mapped.y - dest_bounds.min.y) / dest_h * height_px as f64).round() as i32;
            write_raster_pixel(&mut dest, dest_x, dest_y, pixel);
            write_raster_pixel(&mut dest, dest_x + 1, dest_y, pixel);
            write_raster_pixel(&mut dest, dest_x, dest_y + 1, pixel);
            write_raster_pixel(&mut dest, dest_x + 1, dest_y + 1, pixel);
        }
    }

    let png = encode_raster_png(dest)?;
    let asset = Asset::new(
        format!("{} Deformed.png", object.name),
        AssetMediaType::Png,
        png.len() as u64,
        Some(width_px),
        Some(height_px),
    );
    let new_asset_id = asset.id;
    project.add_asset(asset, png);
    let dest_layer = reroute_image_result_layer(project, object.layer_id)?;
    let object = project
        .find_object_mut(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    object.data = ObjectData::RasterImage {
        asset_key: new_asset_id.to_string(),
        original_width_px: width_px,
        original_height_px: height_px,
        adjustments,
        masks: Vec::new(),
    };
    object.bounds = dest_bounds;
    object.transform = Transform2D::identity();
    object.layer_id = dest_layer;
    object.tabs.clear();
    object.start_point_edits.clear();
    Ok(())
}

fn parse_handle_type(handle_type: Option<&str>) -> ServiceResult<Option<HandleType>> {
    match handle_type {
        Some("in") => Ok(Some(HandleType::In)),
        Some("out") => Ok(Some(HandleType::Out)),
        Some(other) => Err(ServiceError::invalid_input(format!(
            "Invalid handle type: {other}"
        ))),
        None => Ok(None),
    }
}

fn command_endpoint(cmd: &PathCommand) -> Option<Point2D> {
    match *cmd {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => Some(Point2D::new(x, y)),
        PathCommand::Close => None,
    }
}

fn set_command_endpoint(cmd: &mut PathCommand, point: Point2D) -> bool {
    match cmd {
        PathCommand::MoveTo { x, y }
        | PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. } => {
            *x = point.x;
            *y = point.y;
            true
        }
        PathCommand::Close => false,
    }
}

fn endpoint_index_for_subpath(subpath: &SubPath, node_id: NodeId) -> Option<&'static str> {
    if subpath.closed {
        return None;
    }
    let last_idx = subpath
        .commands
        .iter()
        .rposition(|cmd| !matches!(cmd, PathCommand::Close))?;
    if node_id.command_idx == 0 {
        Some("start")
    } else if node_id.command_idx == last_idx {
        Some("end")
    } else {
        None
    }
}

fn segment_start_point(path: &VecPath, node_id: NodeId) -> Option<Point2D> {
    let subpath = path.subpaths.get(node_id.subpath_idx)?;
    if node_id.command_idx == 0 {
        return None;
    }
    command_endpoint(subpath.commands.get(node_id.command_idx - 1)?)
}

fn align_segment_to_angle_in_place(path: &mut VecPath, node_id: NodeId) -> bool {
    let subpath = match path.subpaths.get(node_id.subpath_idx) {
        Some(subpath) => subpath,
        None => return false,
    };
    if !matches!(
        subpath.commands.get(node_id.command_idx),
        Some(PathCommand::LineTo { .. })
    ) {
        return false;
    }
    let Some(start) = segment_start_point(path, node_id) else {
        return false;
    };
    let Some(end) = command_endpoint(&subpath.commands[node_id.command_idx]) else {
        return false;
    };
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len <= 1e-9 {
        return false;
    }

    let snapped_angle =
        (dy.atan2(dx) / (std::f64::consts::FRAC_PI_4)).round() * std::f64::consts::FRAC_PI_4;
    let half = len / 2.0;
    let mid = Point2D::new((start.x + end.x) / 2.0, (start.y + end.y) / 2.0);
    let dir = Point2D::new(snapped_angle.cos(), snapped_angle.sin());
    let new_start = Point2D::new(mid.x - dir.x * half, mid.y - dir.y * half);
    let new_end = Point2D::new(mid.x + dir.x * half, mid.y + dir.y * half);

    if !node_edit::move_node(
        path,
        NodeId {
            subpath_idx: node_id.subpath_idx,
            command_idx: node_id.command_idx - 1,
        },
        new_start,
    ) {
        return false;
    }
    node_edit::move_node(path, node_id, new_end)
}

fn merge_oriented_subpaths(first: &SubPath, second: &SubPath, join_point: Point2D) -> SubPath {
    let mut first = first.clone();
    if let Some(last_cmd) = first
        .commands
        .iter_mut()
        .rev()
        .find(|cmd| !matches!(cmd, PathCommand::Close))
    {
        let _ = set_command_endpoint(last_cmd, join_point);
    }

    let mut commands = first.commands;
    for (idx, cmd) in second.commands.iter().enumerate() {
        if idx == 0 && matches!(cmd, PathCommand::MoveTo { .. }) {
            continue;
        }
        if matches!(cmd, PathCommand::Close) {
            continue;
        }
        commands.push(*cmd);
    }

    SubPath {
        commands,
        closed: false,
    }
}

fn collect_cutter_polylines(
    project: &beambench_core::Project,
    active_object_id: ObjectId,
    active_world_path: &VecPath,
    excluded_subpath_idx: usize,
) -> Vec<(Vec<Point2D>, bool)> {
    let mut cutters = Vec::new();

    for (idx, polyline) in flatten_vecpath(active_world_path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .enumerate()
    {
        if idx == excluded_subpath_idx {
            continue;
        }
        cutters.push((polyline.points, polyline.closed));
    }

    for obj in &project.objects {
        if obj.id == active_object_id {
            continue;
        }
        let Some(vec_path) = object_to_world_vecpath(obj) else {
            continue;
        };
        for polyline in flatten_vecpath(&vec_path, DEFAULT_TOLERANCE_MM) {
            cutters.push((polyline.points, polyline.closed));
        }
    }

    cutters
}

fn endpoint_tangent(path: &VecPath, node_id: NodeId) -> Option<(Point2D, Point2D)> {
    let subpath = path.subpaths.get(node_id.subpath_idx)?;
    let end_kind = endpoint_index_for_subpath(subpath, node_id)?;
    let origin = command_endpoint(subpath.commands.get(node_id.command_idx)?)?;

    let raw = match end_kind {
        "start" => match subpath.commands.get(1)? {
            PathCommand::LineTo { x, y } => Point2D::new(x - origin.x, y - origin.y),
            PathCommand::QuadTo { cx, cy, .. } => Point2D::new(cx - origin.x, cy - origin.y),
            PathCommand::CubicTo { c1x, c1y, .. } => Point2D::new(c1x - origin.x, c1y - origin.y),
            _ => return None,
        },
        "end" => {
            let cmd = subpath.commands.get(node_id.command_idx)?;
            match *cmd {
                PathCommand::LineTo { .. } => {
                    let start = segment_start_point(path, node_id)?;
                    Point2D::new(origin.x - start.x, origin.y - start.y)
                }
                PathCommand::QuadTo { cx, cy, .. } => Point2D::new(origin.x - cx, origin.y - cy),
                PathCommand::CubicTo { c2x, c2y, .. } => {
                    Point2D::new(origin.x - c2x, origin.y - c2y)
                }
                _ => return None,
            }
        }
        _ => return None,
    };

    let len = (raw.x * raw.x + raw.y * raw.y).sqrt();
    if len <= 1e-9 {
        return None;
    }
    Some((origin, Point2D::new(raw.x / len, raw.y / len)))
}

fn ray_segment_intersection(
    origin: Point2D,
    direction: Point2D,
    a: Point2D,
    b: Point2D,
) -> Option<(f64, Point2D)> {
    let ray_dx = direction.x;
    let ray_dy = direction.y;
    let seg_dx = b.x - a.x;
    let seg_dy = b.y - a.y;
    let denom = ray_dx * seg_dy - ray_dy * seg_dx;
    if denom.abs() <= 1e-12 {
        return None;
    }

    let ox = a.x - origin.x;
    let oy = a.y - origin.y;
    let t = (ox * seg_dy - oy * seg_dx) / denom;
    let u = (ox * ray_dy - oy * ray_dx) / denom;
    if t <= 1e-6 || !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some((
        t,
        Point2D::new(origin.x + ray_dx * t, origin.y + ray_dy * t),
    ))
}

pub(crate) fn convert_to_path_in_project(
    project: &mut Project,
    input: ConvertToPathInput,
) -> ServiceResult<ProjectObject> {
    let was_dirty = project.dirty;
    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let vec_path = object_to_world_vecpath(obj)
        .ok_or_else(|| ServiceError::invalid_input("Cannot convert this object type to a path"))?;
    let current_layer = obj.layer_id;

    let new_bounds = vec_path.visual_bounds().unwrap_or(obj.bounds);
    let path_data = vec_path.to_svg_d();
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);

    {
        let obj = project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        obj.data = ObjectData::VectorPath {
            path_data,
            closed,
            ruler_guide_axis: None,
        };
        obj.bounds = new_bounds;
        obj.transform = Transform2D::identity();
        obj.start_point_edits.clear();
    }
    let dest_layer = reroute_vector_result_layer(project, current_layer)?;
    if dest_layer != current_layer {
        let obj = project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        obj.layer_id = dest_layer;
    }
    let result = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?
        .clone();
    project.dirty = was_dirty;
    Ok(result)
}

pub fn convert_to_path(
    ctx: &ServiceContext,
    input: ConvertToPathInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Snapshot first, then auto-unlink VirtualClone if needed
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let result = convert_to_path_in_project(project, input)?;
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.converted_to_path",
        json!({
            "object": events::object_summary(&result),
        }),
    );
    Ok(result)
}

pub fn mesh_deform_selection(
    ctx: &ServiceContext,
    input: MeshDeformSelectionInput,
) -> ServiceResult<Vec<ProjectObject>> {
    if input.object_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mapper = MeshDeformMapper::new(&input)?;
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let mut updated_ids = Vec::new();
    let mut seen = HashSet::new();
    for object_id in input.object_ids {
        if !seen.insert(object_id) {
            continue;
        }

        project
            .ensure_resolved(object_id)
            .map_err(ServiceError::internal)?;

        let is_raster = {
            let obj = project
                .find_object(object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            if obj.locked {
                return Err(ServiceError::invalid_input("Object is locked"));
            }
            matches!(obj.data, ObjectData::RasterImage { .. })
        };
        if is_raster {
            deform_raster_object(project, object_id, &mapper)?;
            updated_ids.push(object_id);
            continue;
        }

        let (current_layer, source_path) = {
            let obj = project
                .find_object(object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            let source_path = object_to_world_vecpath(obj).ok_or_else(|| {
                ServiceError::invalid_input("Warp and Deform require vector-compatible objects")
            })?;
            (obj.layer_id, source_path)
        };

        let deformed = deform_vec_path(&source_path, &mapper);
        let dest_layer = reroute_vector_result_layer(project, current_layer)?;
        {
            let obj = project
                .find_object_mut(object_id)
                .ok_or_else(|| ServiceError::not_found("Object not found"))?;
            write_vec_path_to_object(obj, &deformed, None);
            obj.transform = Transform2D::identity();
            obj.start_point_edits.clear();
            obj.layer_id = dest_layer;
        }
        updated_ids.push(object_id);
    }

    project.dirty = true;
    let updated: Vec<ProjectObject> = updated_ids
        .iter()
        .filter_map(|id| project.find_object(*id).cloned())
        .collect();

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.mesh_deformed",
        json!({
            "object_ids": updated_ids.iter().map(ToString::to_string).collect::<Vec<_>>(),
            "grid_size": mapper.grid_size,
            "perspective": mapper.perspective,
            "objects": updated.iter().map(events::object_summary).collect::<Vec<_>>(),
        }),
    );
    Ok(updated)
}

struct BooleanOperand {
    path: VecPath,
    layer_id: beambench_core::LayerId,
    delete_ids: Vec<ObjectId>,
}

fn boolean_event_name(op_name: &str) -> &'static str {
    match op_name {
        "Union" => "vector.boolean.union",
        "Subtract" => "vector.boolean.subtract",
        "Intersection" => "vector.boolean.intersection",
        "Weld" => "vector.boolean.weld",
        "Exclude" => "vector.boolean.exclude",
        _ => "vector.boolean.op",
    }
}

fn path_is_closed_boolean_operand(path: &VecPath) -> bool {
    !path.subpaths.is_empty() && path.subpaths.iter().all(|sp| sp.closed)
}

fn path_contains_point_evenodd(path: &VecPath, point: Point2D) -> bool {
    let mut filled = false;
    for polyline in flatten_vecpath(path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .filter(|polyline| polyline.points.len() >= 3)
    {
        let mut inside = false;
        let points = &polyline.points;
        let mut j = points.len() - 1;
        for i in 0..points.len() {
            let pi = &points[i];
            let pj = &points[j];
            if (pi.y > point.y) != (pj.y > point.y) {
                let x_at_y = (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y) + pi.x;
                if point.x < x_at_y {
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

fn bounds_center(bounds: Bounds) -> Point2D {
    Point2D::new(
        (bounds.min.x + bounds.max.x) * 0.5,
        (bounds.min.y + bounds.max.y) * 0.5,
    )
}

struct BooleanOperandPathInfo {
    index: usize,
    path: VecPath,
    bounds: Option<Bounds>,
    depth: usize,
}

// Group leaves are separate objects: sibling overlaps union together, while
// strict containment alternates as holes/islands to preserve even-odd grouping.
fn combine_boolean_operand_paths(paths: Vec<VecPath>) -> VecPath {
    if paths.len() == 1 {
        return paths.into_iter().next().unwrap_or_else(VecPath::new);
    }

    let mut infos: Vec<BooleanOperandPathInfo> = paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| BooleanOperandPathInfo {
            index,
            bounds: path.visual_bounds(),
            path,
            depth: 0,
        })
        .collect();

    for i in 0..infos.len() {
        let Some(inner_bounds) = infos[i].bounds else {
            continue;
        };
        let inner_area = bounds_area(inner_bounds);
        let sample = bounds_center(inner_bounds);

        let depth = infos
            .iter()
            .enumerate()
            .filter(|(j, outer)| {
                if i == *j {
                    return false;
                }
                let Some(outer_bounds) = outer.bounds else {
                    return false;
                };
                bounds_area(outer_bounds) > inner_area
                    && bounds_contains(outer_bounds, inner_bounds)
                    && path_contains_point_evenodd(&outer.path, sample)
            })
            .count();

        infos[i].depth = depth;
    }

    infos.sort_by_key(|info| (info.depth, info.index));

    let mut result: Option<VecPath> = None;
    for info in infos {
        if info.depth % 2 == 0 {
            result = Some(match result {
                Some(existing) => path_union(&existing, &info.path),
                None => info.path,
            });
        } else if let Some(existing) = result {
            result = Some(path_subtract(&existing, &info.path));
        }
    }

    result.unwrap_or_else(VecPath::new)
}

fn append_operand_paths(
    project: &Project,
    obj: &ProjectObject,
    seen: &mut HashSet<ObjectId>,
    paths: &mut Vec<VecPath>,
    delete_ids: &mut Vec<ObjectId>,
) -> ServiceResult<()> {
    if !seen.insert(obj.id) {
        return Err(ServiceError::invalid_input(
            "Group contains a recursive object reference",
        ));
    }
    if !obj.visible {
        return Err(ServiceError::invalid_input(format!(
            "{} is hidden and cannot be used for boolean operations",
            obj.name
        )));
    }
    if obj.locked {
        return Err(ServiceError::invalid_input(format!(
            "{} is locked and cannot be used for boolean operations",
            obj.name
        )));
    }

    delete_ids.push(obj.id);
    let effective = project.resolve_clone(obj).unwrap_or_else(|| obj.clone());
    match &effective.data {
        ObjectData::Group { children } => {
            if children.is_empty() {
                return Err(ServiceError::invalid_input(format!(
                    "{} is an empty group",
                    obj.name
                )));
            }
            for child_id in children {
                let child = project
                    .find_object(*child_id)
                    .ok_or_else(|| ServiceError::not_found("Group child not found"))?;
                append_operand_paths(project, child, seen, paths, delete_ids)?;
            }
        }
        _ => {
            let path = object_to_world_vecpath_resolved(obj, project).ok_or_else(|| {
                ServiceError::invalid_input(format!(
                    "{} is not a vector-compatible boolean operand",
                    obj.name
                ))
            })?;
            if !path_is_closed_boolean_operand(&path) {
                return Err(ServiceError::invalid_input(format!(
                    "{} is not a closed boolean operand",
                    obj.name
                )));
            }
            paths.push(path);
        }
    }
    Ok(())
}

fn resolve_boolean_operand(
    project: &Project,
    object_id: ObjectId,
    missing_message: &str,
) -> ServiceResult<BooleanOperand> {
    let obj = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found(missing_message))?;
    let mut paths = Vec::new();
    let mut delete_ids = Vec::new();
    let mut seen = HashSet::new();
    append_operand_paths(project, obj, &mut seen, &mut paths, &mut delete_ids)?;

    if paths.is_empty() {
        return Err(ServiceError::invalid_input(format!(
            "{} has no vector-compatible boolean operands",
            obj.name
        )));
    }

    let path = combine_boolean_operand_paths(paths);

    Ok(BooleanOperand {
        path,
        layer_id: obj.layer_id,
        delete_ids,
    })
}

pub fn boolean_operand_path_for_preview(
    project: &Project,
    object_id: ObjectId,
) -> ServiceResult<VecPath> {
    Ok(resolve_boolean_operand(project, object_id, "Object not found")?.path)
}

fn dedupe_ids(ids: impl IntoIterator<Item = ObjectId>) -> Vec<ObjectId> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for id in ids {
        if seen.insert(id) {
            result.push(id);
        }
    }
    result
}

fn create_boolean_result(
    project: &mut Project,
    op_name: &str,
    requested_layer_id: beambench_core::LayerId,
    result: VecPath,
) -> ServiceResult<ProjectObject> {
    let layer_id = reroute_vector_result_layer(project, requested_layer_id)?;
    let result_bounds = result
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let path_data = result.to_svg_d();
    let closed = result.subpaths.iter().any(|sp| sp.closed);
    let new_obj = ProjectObject::new(
        op_name,
        layer_id,
        result_bounds,
        ObjectData::VectorPath {
            path_data,
            closed,
            ruler_guide_axis: None,
        },
    );
    Ok(project.add_object(new_obj).clone())
}

fn boolean_binary_op(
    ctx: &ServiceContext,
    input: BooleanOpInput,
    op_name: &str,
    op: fn(&VecPath, &VecPath) -> VecPath,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Snapshot first, then auto-unlink VirtualClones
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let result_obj = boolean_binary_op_in_project(project, input.clone(), op_name, op)?;
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        boolean_event_name(op_name),
        json!({
            "source_object_ids": [input.object_id_a, input.object_id_b],
            "object": events::object_summary(&result_obj),
        }),
    );
    Ok(result_obj)
}

pub(crate) fn boolean_binary_op_in_project(
    project: &mut Project,
    input: BooleanOpInput,
    op_name: &str,
    op: fn(&VecPath, &VecPath) -> VecPath,
) -> ServiceResult<ProjectObject> {
    let was_dirty = project.dirty;
    project
        .ensure_resolved(input.object_id_a)
        .map_err(ServiceError::internal)?;
    project
        .ensure_resolved(input.object_id_b)
        .map_err(ServiceError::internal)?;

    let operand_a = resolve_boolean_operand(project, input.object_id_a, "Object A not found")?;
    let operand_b = resolve_boolean_operand(project, input.object_id_b, "Object B not found")?;
    let requested_layer_id = operand_a.layer_id;
    let result = op(&operand_a.path, &operand_b.path);
    let delete_ids = dedupe_ids(operand_a.delete_ids.into_iter().chain(operand_b.delete_ids));
    project.remove_objects(&delete_ids);
    let result_obj = create_boolean_result(project, op_name, requested_layer_id, result)?;
    project.dirty = was_dirty;
    Ok(result_obj)
}

pub fn boolean_union(ctx: &ServiceContext, input: BooleanOpInput) -> ServiceResult<ProjectObject> {
    boolean_binary_op(ctx, input, "Union", path_union)
}

pub fn boolean_subtract(
    ctx: &ServiceContext,
    input: BooleanOpInput,
) -> ServiceResult<ProjectObject> {
    boolean_binary_op(ctx, input, "Subtract", path_subtract)
}

pub fn boolean_intersection(
    ctx: &ServiceContext,
    input: BooleanOpInput,
) -> ServiceResult<ProjectObject> {
    boolean_binary_op(ctx, input, "Intersection", path_intersection)
}

pub fn boolean_exclude(
    ctx: &ServiceContext,
    input: BooleanOpInput,
) -> ServiceResult<ProjectObject> {
    boolean_binary_op(ctx, input, "Exclude", path_exclude)
}

pub fn boolean_weld(ctx: &ServiceContext, input: BooleanWeldInput) -> ServiceResult<ProjectObject> {
    if input.object_ids.len() < 2 {
        return Err(ServiceError::invalid_input(
            "Weld requires at least two objects",
        ));
    }

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let result_obj = boolean_weld_in_project(project, input.clone())?;
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        boolean_event_name("Weld"),
        json!({
            "source_object_ids": input.object_ids,
            "object": events::object_summary(&result_obj),
        }),
    );
    Ok(result_obj)
}

pub(crate) fn boolean_weld_in_project(
    project: &mut Project,
    input: BooleanWeldInput,
) -> ServiceResult<ProjectObject> {
    let was_dirty = project.dirty;
    if input.object_ids.len() < 2 {
        return Err(ServiceError::invalid_input(
            "Weld requires at least two objects",
        ));
    }
    for object_id in &input.object_ids {
        project
            .ensure_resolved(*object_id)
            .map_err(ServiceError::internal)?;
    }

    let mut operands = Vec::new();
    for object_id in &input.object_ids {
        operands.push(resolve_boolean_operand(
            project,
            *object_id,
            "Object not found",
        )?);
    }
    let requested_layer_id = operands
        .first()
        .ok_or_else(|| ServiceError::invalid_input("Weld requires at least two objects"))?
        .layer_id;
    let paths: Vec<VecPath> = operands
        .iter()
        .map(|operand| operand.path.clone())
        .collect();
    let delete_ids = dedupe_ids(operands.into_iter().flat_map(|operand| operand.delete_ids));
    let result = weld_shapes(&paths);
    project.remove_objects(&delete_ids);
    let result_obj = create_boolean_result(project, "Weld", requested_layer_id, result)?;
    project.dirty = was_dirty;
    Ok(result_obj)
}

pub fn group_objects(
    ctx: &ServiceContext,
    input: GroupObjectsInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let result = group_objects_in_project(project, input.clone())?;
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.group.created",
        json!({
            "group": events::object_summary(&result),
            "children": input.object_ids,
        }),
    );
    Ok(result)
}

pub(crate) fn group_objects_in_project(
    project: &mut Project,
    input: GroupObjectsInput,
) -> ServiceResult<ProjectObject> {
    group_objects_named_in_project(project, input.object_ids, "Group")
}

fn group_objects_named_in_project(
    project: &mut Project,
    object_ids: Vec<ObjectId>,
    name: &str,
) -> ServiceResult<ProjectObject> {
    let was_dirty = project.dirty;
    if object_ids.len() < 2 {
        return Err(ServiceError::invalid_input(
            "Need at least 2 objects to group",
        ));
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut layer_id = None;

    for &id in &object_ids {
        let obj = project
            .find_object(id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        if layer_id.is_none() {
            layer_id = Some(obj.layer_id);
        }
        min_x = min_x.min(obj.bounds.min.x);
        min_y = min_y.min(obj.bounds.min.y);
        max_x = max_x.max(obj.bounds.max.x);
        max_y = max_y.max(obj.bounds.max.y);
    }

    let group_bounds = Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y));

    // Group is a non-raster aggregation object, so its own
    // `layer_id` must live on a non-image layer per the layer-content
    // invariant. If the children share an image layer, route the
    // Group metadata to a matching non-image sibling. Children keep
    // their own layer assignments.
    let requested_layer =
        layer_id.ok_or_else(|| ServiceError::invalid_input("Need at least 2 objects to group"))?;
    let (group_layer_id, _rerouted) = crate::validation::resolve_layer_for_object(
        project,
        requested_layer,
        crate::validation::RoutingTarget::NeedsNonImage,
    )?;

    let group = ProjectObject::new(
        name,
        group_layer_id,
        group_bounds,
        ObjectData::Group {
            children: object_ids.clone(),
        },
    );
    let result = group.clone();
    project.add_object(group);
    project.dirty = was_dirty;
    Ok(result)
}

fn is_auto_group_guide(object: &ProjectObject) -> bool {
    matches!(
        object.data,
        ObjectData::VectorPath {
            ruler_guide_axis: Some(_),
            ..
        }
    )
}

fn is_auto_group_outer_candidate(object: &ProjectObject) -> bool {
    if !object.visible || object.locked || is_auto_group_guide(object) {
        return false;
    }
    match &object.data {
        ObjectData::VectorPath { closed, .. } => *closed,
        ObjectData::Shape { .. } | ObjectData::Star { .. } | ObjectData::Polygon { .. } => true,
        ObjectData::Text { .. }
        | ObjectData::RasterImage { .. }
        | ObjectData::Barcode { .. }
        | ObjectData::Group { .. }
        | ObjectData::VirtualClone { .. } => false,
    }
}

fn is_auto_group_child_candidate(object: &ProjectObject) -> bool {
    object.visible && !object.locked && !is_auto_group_guide(object)
}

fn bounds_contains(outer: Bounds, inner: Bounds) -> bool {
    inner.min.x >= outer.min.x
        && inner.max.x <= outer.max.x
        && inner.min.y >= outer.min.y
        && inner.max.y <= outer.max.y
}

fn bounds_area(bounds: Bounds) -> f64 {
    (bounds.max.x - bounds.min.x).abs() * (bounds.max.y - bounds.min.y).abs()
}

fn auto_group_candidates(project: &Project, object_ids: &[ObjectId]) -> Vec<Vec<ObjectId>> {
    if object_ids.len() < 2 {
        return Vec::new();
    }
    let selected: HashSet<ObjectId> = object_ids.iter().copied().collect();
    let selected_objects: Vec<&ProjectObject> = object_ids
        .iter()
        .filter_map(|id| project.find_object(*id))
        .collect();
    let outers: Vec<&ProjectObject> = selected_objects
        .iter()
        .copied()
        .filter(|object| is_auto_group_outer_candidate(object))
        .collect();
    if outers.is_empty() {
        return Vec::new();
    }

    let mut children_by_outer: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    for child in selected_objects {
        if !is_auto_group_child_candidate(child) {
            continue;
        }
        let mut containers: Vec<&ProjectObject> = outers
            .iter()
            .copied()
            .filter(|outer| outer.id != child.id)
            .filter(|outer| selected.contains(&outer.id))
            .filter(|outer| bounds_contains(outer.bounds, child.bounds))
            .collect();
        containers.sort_by(|a, b| {
            bounds_area(a.bounds)
                .partial_cmp(&bounds_area(b.bounds))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        if let Some(outer) = containers.first() {
            children_by_outer
                .entry(outer.id)
                .or_default()
                .push(child.id);
        }
    }

    outers
        .into_iter()
        .filter_map(|outer| {
            let children = children_by_outer.remove(&outer.id)?;
            if children.is_empty() {
                return None;
            }
            let mut ids = Vec::with_capacity(children.len() + 1);
            ids.push(outer.id);
            ids.extend(children);
            Some(ids)
        })
        .collect()
}

pub fn auto_group_objects(
    ctx: &ServiceContext,
    input: AutoGroupObjectsInput,
) -> ServiceResult<Vec<ProjectObject>> {
    let object_ids = input.object_ids;
    if object_ids.len() < 2 {
        return Ok(Vec::new());
    }

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let candidates = auto_group_candidates(project, &object_ids);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let mut groups = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let group = group_objects_named_in_project(project, candidate, "Auto-Group")?;
        groups.push(group);
    }
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.auto_group.created",
        json!({
            "groups": groups.iter().map(events::object_summary).collect::<Vec<_>>(),
            "source_object_ids": object_ids,
        }),
    );
    Ok(groups)
}

pub fn ungroup_objects(ctx: &ServiceContext, group_id: ObjectId) -> ServiceResult<Vec<ObjectId>> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let children = ungroup_objects_in_project(project, group_id)?;
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.group.removed",
        json!({
            "group_id": group_id,
            "children": children,
        }),
    );
    Ok(children)
}

pub(crate) fn ungroup_objects_in_project(
    project: &mut Project,
    group_id: ObjectId,
) -> ServiceResult<Vec<ObjectId>> {
    let was_dirty = project.dirty;
    let obj = project
        .find_object(group_id)
        .ok_or_else(|| ServiceError::not_found("Group not found"))?;
    let children = match &obj.data {
        ObjectData::Group { children } => children.clone(),
        _ => return Err(ServiceError::invalid_input("Object is not a group")),
    };
    project.remove_object(group_id);
    project.dirty = was_dirty;
    Ok(children)
}

pub fn get_editable_path(
    ctx: &ServiceContext,
    object_id: ObjectId,
) -> ServiceResult<Vec<EditablePath>> {
    let guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;
    let obj = project
        .find_object(object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    // Resolve VirtualClone to its source geometry (transient, no mutation)
    let resolved = project.resolve_clone(obj);
    let effective = resolved.as_ref().unwrap_or(obj);
    let mut vec_path = require_vector_path(effective)?;
    // Transient denormalization — show clean path in node-edit mode
    for entry in &effective.start_point_edits {
        if entry.normalized && entry.subpath_index < vec_path.subpaths.len() {
            vec_path.subpaths[entry.subpath_index] =
                path_ops_core::denormalize_closed_subpath(&vec_path.subpaths[entry.subpath_index]);
        }
    }
    Ok(EditablePath::from_vecpath(&vec_path))
}

pub fn update_node(ctx: &ServiceContext, input: UpdateNodeInput) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );

    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;

    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };
    let new_pos = Point2D::new(input.x, input.y);
    let success = match parse_handle_type(input.handle_type.as_deref())? {
        Some(handle_type) => node_edit::move_handle(&mut vec_path, node_id, handle_type, new_pos),
        None => node_edit::move_node(&mut vec_path, node_id, new_pos),
    };

    if !success {
        return Err(ServiceError::invalid_input("Failed to update node"));
    }

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.node.updated",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
            "handle_type": input.handle_type,
        }),
    );
    Ok(result)
}

pub fn update_nodes_batch(
    ctx: &ServiceContext,
    input: UpdateNodesBatchInput,
) -> ServiceResult<ProjectObject> {
    if input.updates.is_empty() {
        return Err(ServiceError::invalid_input("No node updates supplied"));
    }

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );

    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;

    for update in &input.updates {
        let new_pos = Point2D::new(update.x, update.y);
        let moved = match parse_handle_type(update.handle_type.as_deref())? {
            Some(handle_type) => {
                node_edit::move_handle(&mut vec_path, update.node_id, handle_type, new_pos)
            }
            None => node_edit::move_node(&mut vec_path, update.node_id, new_pos),
        };
        if !moved {
            return Err(ServiceError::invalid_input(
                "Failed to update one or more nodes",
            ));
        }
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.nodes.updated_batch",
        json!({
            "object": events::object_summary(&result),
            "update_count": input.updates.len(),
        }),
    );
    Ok(result)
}

pub fn set_node_type(
    ctx: &ServiceContext,
    input: SetNodeTypeInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };
    let node_type = match input.node_type.as_str() {
        "smooth" => NodeType::Smooth,
        "corner" => NodeType::Corner,
        other => {
            return Err(ServiceError::invalid_input(format!(
                "Invalid node type: {other}"
            )));
        }
    };

    if !node_edit::set_node_type(&mut vec_path, node_id, node_type) {
        return Err(ServiceError::invalid_input("Node type is already set"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.node.type_set",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
            "node_type": input.node_type,
        }),
    );
    Ok(result)
}

pub fn delete_node(ctx: &ServiceContext, input: DeleteNodeInput) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::delete_node(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input("Cannot delete this node"));
    }
    prune_degenerate_subpaths(&mut vec_path);
    if vec_path.subpaths.is_empty() {
        return Err(ServiceError::invalid_input(
            "Cannot delete every node; delete the object instead",
        ));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.node.deleted",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
        }),
    );
    Ok(result)
}

pub fn delete_nodes(ctx: &ServiceContext, input: DeleteNodesInput) -> ServiceResult<ProjectObject> {
    if input.node_ids.is_empty() {
        return Err(ServiceError::invalid_input("No nodes supplied"));
    }

    let mut node_ids = input.node_ids;
    node_ids.sort_by(|a, b| {
        b.subpath_idx
            .cmp(&a.subpath_idx)
            .then_with(|| b.command_idx.cmp(&a.command_idx))
    });
    node_ids.dedup_by(|a, b| a.subpath_idx == b.subpath_idx && a.command_idx == b.command_idx);

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let delete_keys: HashSet<(usize, usize)> = node_ids
        .iter()
        .map(|node_id| (node_id.subpath_idx, node_id.command_idx))
        .collect();
    let fully_selected_subpaths: HashSet<usize> = vec_path
        .subpaths
        .iter()
        .enumerate()
        .filter_map(|(subpath_idx, subpath)| {
            let editable_command_idxs: Vec<usize> = subpath
                .commands
                .iter()
                .enumerate()
                .filter_map(|(command_idx, command)| {
                    command_has_editable_node(command).then_some(command_idx)
                })
                .collect();
            if editable_command_idxs.is_empty() {
                return None;
            }
            editable_command_idxs
                .iter()
                .all(|command_idx| delete_keys.contains(&(subpath_idx, *command_idx)))
                .then_some(subpath_idx)
        })
        .collect();

    for node_id in &node_ids {
        if fully_selected_subpaths.contains(&node_id.subpath_idx) {
            continue;
        }
        if !node_edit::delete_node(&mut vec_path, *node_id) {
            return Err(ServiceError::invalid_input(
                "Cannot delete one or more selected nodes",
            ));
        }
    }
    if !fully_selected_subpaths.is_empty() {
        vec_path.subpaths = vec_path
            .subpaths
            .into_iter()
            .enumerate()
            .filter_map(|(subpath_idx, subpath)| {
                (!fully_selected_subpaths.contains(&subpath_idx)).then_some(subpath)
            })
            .collect();
    }
    prune_degenerate_subpaths(&mut vec_path);
    if vec_path.subpaths.is_empty() {
        return Err(ServiceError::invalid_input(
            "Cannot delete every node; delete the object instead",
        ));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.nodes.deleted_batch",
        json!({
            "object": events::object_summary(&result),
            "delete_count": node_ids.len(),
        }),
    );
    Ok(result)
}

pub fn insert_node(ctx: &ServiceContext, input: InsertNodeInput) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::insert_node(&mut vec_path, node_id, input.t) {
        return Err(ServiceError::invalid_input("Cannot insert node here"));
    }

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.node.inserted",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
            "t": input.t,
        }),
    );
    Ok(result)
}

pub fn convert_segment_to_line(
    ctx: &ServiceContext,
    input: SegmentOpInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Prepare and validate before pushing undo snapshot
    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::convert_segment_to_line(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input("Segment is already a line"));
    }

    // Operation will succeed — push undo snapshot now
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.segment.converted",
        json!({ "object": events::object_summary(&result), "to": "line" }),
    );
    Ok(result)
}

pub fn convert_segment_to_curve(
    ctx: &ServiceContext,
    input: SegmentOpInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::convert_segment_to_curve(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input("Segment is already a curve"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.segment.converted",
        json!({ "object": events::object_summary(&result), "to": "curve" }),
    );
    Ok(result)
}

pub fn delete_segment(ctx: &ServiceContext, input: SegmentOpInput) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::delete_segment(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input("Cannot delete this segment"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let new_bounds = vec_path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    obj.data = ObjectData::VectorPath {
        path_data: vec_path.to_svg_d(),
        closed,
        ruler_guide_axis: None,
    };
    obj.bounds = new_bounds;
    obj.tabs.clear();
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.segment.deleted",
        json!({ "object": events::object_summary(&result) }),
    );
    Ok(result)
}

pub fn break_path_at_node(
    ctx: &ServiceContext,
    input: SegmentOpInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !node_edit::break_path_at_node(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input("Cannot break path here"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let new_bounds = vec_path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    obj.data = ObjectData::VectorPath {
        path_data: vec_path.to_svg_d(),
        closed,
        ruler_guide_axis: None,
    };
    obj.bounds = new_bounds;
    obj.tabs.clear();
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.path.broken",
        json!({ "object": events::object_summary(&result) }),
    );
    Ok(result)
}

pub fn toggle_path_closed(
    ctx: &ServiceContext,
    input: SubpathOpInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;

    if !node_edit::toggle_path_closed(&mut vec_path, input.subpath_idx) {
        return Err(ServiceError::invalid_input("Invalid subpath index"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.path.toggled_closed",
        json!({ "object": events::object_summary(&result) }),
    );
    Ok(result)
}

pub fn align_segment_to_angle(
    ctx: &ServiceContext,
    input: SegmentOpInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let node_id = NodeId {
        subpath_idx: input.subpath_idx,
        command_idx: input.command_idx,
    };

    if !align_segment_to_angle_in_place(&mut vec_path, node_id) {
        return Err(ServiceError::invalid_input(
            "Only straight line segments can be aligned to angle",
        ));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.segment.aligned_to_angle",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
        }),
    );
    Ok(result)
}

pub fn trim_segment_to_intersection(
    ctx: &ServiceContext,
    input: SegmentOpInput,
    click_x: f64,
    click_y: f64,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let world_path = object_to_world_vecpath(obj)
        .ok_or_else(|| ServiceError::invalid_input("Object is not a vector type"))?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let cutters =
        collect_cutter_polylines(project, input.object_id, &world_path, input.subpath_idx);
    let trimmed = trim_core::trim_at_intersection(
        &world_path,
        input.subpath_idx,
        &cutters,
        Point2D::new(click_x, click_y),
    );
    if trimmed.pieces.is_empty() {
        return Err(ServiceError::invalid_input(
            "No intersection available to trim",
        ));
    }

    let mut new_path = VecPath {
        subpaths: Vec::new(),
    };
    for (idx, subpath) in world_path.subpaths.iter().enumerate() {
        if idx != input.subpath_idx {
            new_path.subpaths.push(subpath.clone());
        }
    }
    for piece in trimmed.pieces {
        new_path.subpaths.extend(piece.subpaths);
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &new_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.segment.trimmed_to_intersection",
        json!({
            "object": events::object_summary(&result),
            "subpath_idx": input.subpath_idx,
            "command_idx": input.command_idx,
            "click_x": click_x,
            "click_y": click_y,
        }),
    );
    Ok(result)
}

pub fn extend_endpoint_to_intersection(
    ctx: &ServiceContext,
    input: ExtendEndpointInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = object_to_world_vecpath(obj)
        .ok_or_else(|| ServiceError::invalid_input("Object is not a vector type"))?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;
    let (origin, direction) = endpoint_tangent(&vec_path, input.node_id).ok_or_else(|| {
        ServiceError::invalid_input("Extend to intersection requires an open-path endpoint")
    })?;

    let mut best_hit: Option<(f64, Point2D)> = None;

    for (idx, polyline) in flatten_vecpath(&vec_path, DEFAULT_TOLERANCE_MM)
        .into_iter()
        .enumerate()
    {
        if idx == input.node_id.subpath_idx || polyline.points.len() < 2 {
            continue;
        }
        let seg_count = if polyline.closed {
            polyline.points.len()
        } else {
            polyline.points.len() - 1
        };
        for seg_idx in 0..seg_count {
            let start = polyline.points[seg_idx];
            let end = polyline.points[(seg_idx + 1) % polyline.points.len()];
            if let Some(hit) = ray_segment_intersection(origin, direction, start, end) {
                if best_hit.is_none_or(|current| hit.0 < current.0) {
                    best_hit = Some(hit);
                }
            }
        }
    }

    for other in &project.objects {
        if other.id == input.object_id {
            continue;
        }
        let Some(other_path) = object_to_world_vecpath(other) else {
            continue;
        };
        for polyline in flatten_vecpath(&other_path, DEFAULT_TOLERANCE_MM) {
            if polyline.points.len() < 2 {
                continue;
            }
            let seg_count = if polyline.closed {
                polyline.points.len()
            } else {
                polyline.points.len() - 1
            };
            for seg_idx in 0..seg_count {
                let start = polyline.points[seg_idx];
                let end = polyline.points[(seg_idx + 1) % polyline.points.len()];
                if let Some(hit) = ray_segment_intersection(origin, direction, start, end) {
                    if best_hit.is_none_or(|current| hit.0 < current.0) {
                        best_hit = Some(hit);
                    }
                }
            }
        }
    }

    let Some((_, hit_point)) = best_hit else {
        return Err(ServiceError::invalid_input(
            "No forward intersection found for this endpoint",
        ));
    };

    if !node_edit::move_node(&mut vec_path, input.node_id, hit_point) {
        return Err(ServiceError::invalid_input("Failed to extend endpoint"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.endpoint.extended_to_intersection",
        json!({
            "object": events::object_summary(&result),
            "node_id": input.node_id,
        }),
    );
    Ok(result)
}

pub fn join_subpaths(
    ctx: &ServiceContext,
    input: JoinSubpathsInput,
) -> ServiceResult<ProjectObject> {
    if input.src_node_id == input.dst_node_id {
        return Err(ServiceError::invalid_input(
            "Cannot join an endpoint to itself",
        ));
    }

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );
    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;
    let ruler_guide_axis = get_vector_path_meta(obj)?;

    let src_subpath = vec_path
        .subpaths
        .get(input.src_node_id.subpath_idx)
        .ok_or_else(|| ServiceError::invalid_input("Invalid source endpoint"))?;
    let dst_subpath = vec_path
        .subpaths
        .get(input.dst_node_id.subpath_idx)
        .ok_or_else(|| ServiceError::invalid_input("Invalid destination endpoint"))?;
    if src_subpath.closed || dst_subpath.closed {
        return Err(ServiceError::invalid_input(
            "Only open subpaths can be joined by dragging endpoints",
        ));
    }
    if input.src_node_id.subpath_idx == input.dst_node_id.subpath_idx {
        return Err(ServiceError::invalid_input(
            "Join by dragging only supports distinct open subpaths",
        ));
    }

    let src_kind = endpoint_index_for_subpath(src_subpath, input.src_node_id)
        .ok_or_else(|| ServiceError::invalid_input("Source node is not an open-path endpoint"))?;
    let dst_kind = endpoint_index_for_subpath(dst_subpath, input.dst_node_id).ok_or_else(|| {
        ServiceError::invalid_input("Destination node is not an open-path endpoint")
    })?;

    let first = if src_kind == "end" {
        src_subpath.clone()
    } else {
        trim_core::reverse_subpath(src_subpath)
    };
    let second = if dst_kind == "start" {
        dst_subpath.clone()
    } else {
        trim_core::reverse_subpath(dst_subpath)
    };
    let join_point = trim_core::subpath_first_point(&second)
        .ok_or_else(|| ServiceError::invalid_input("Destination subpath has no join point"))?;
    let merged = merge_oriented_subpaths(&first, &second, join_point);

    let mut new_subpaths = Vec::with_capacity(vec_path.subpaths.len() - 1);
    let keep_idx = input
        .src_node_id
        .subpath_idx
        .min(input.dst_node_id.subpath_idx);
    let drop_idx = input
        .src_node_id
        .subpath_idx
        .max(input.dst_node_id.subpath_idx);
    for (idx, subpath) in vec_path.subpaths.into_iter().enumerate() {
        if idx == keep_idx {
            new_subpaths.push(merged.clone());
        } else if idx == drop_idx {
            continue;
        } else {
            new_subpaths.push(subpath);
        }
    }
    vec_path.subpaths = new_subpaths;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    write_vec_path_to_object(obj, &vec_path, ruler_guide_axis);
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.subpaths.joined",
        json!({
            "object": events::object_summary(&result),
            "src_node_id": input.src_node_id,
            "dst_node_id": input.dst_node_id,
        }),
    );
    Ok(result)
}

pub fn scale_path_to_bounds(
    ctx: &ServiceContext,
    input: ScalePathToBoundsInput,
) -> ServiceResult<ProjectObject> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    // Snapshot first, then auto-unlink VirtualClone
    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    project
        .ensure_resolved(input.object_id)
        .map_err(ServiceError::internal)?;
    path_ops_core::ensure_denormalized(
        project
            .find_object_mut(input.object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?,
    );

    let obj = project
        .find_object(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    let mut vec_path = require_vector_path(obj)?;

    let intrinsic = vec_path
        .visual_bounds()
        .unwrap_or(Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(0.0, 0.0)));
    let old_w = intrinsic.max.x - intrinsic.min.x;
    let old_h = intrinsic.max.y - intrinsic.min.y;
    let new_w = input.new_max_x - input.new_min_x;
    let new_h = input.new_max_y - input.new_min_y;

    let sx = if old_w > 0.0 { new_w / old_w } else { 1.0 };
    let sy = if old_h > 0.0 { new_h / old_h } else { 1.0 };
    let transform = Transform2D {
        a: sx,
        b: 0.0,
        c: 0.0,
        d: sy,
        tx: input.new_min_x - intrinsic.min.x * sx,
        ty: input.new_min_y - intrinsic.min.y * sy,
    };
    vec_path = bake_transform(&vec_path, &transform);

    let result_bounds = Bounds::new(
        Point2D::new(input.new_min_x, input.new_min_y),
        Point2D::new(input.new_max_x, input.new_max_y),
    );
    let closed = vec_path.subpaths.iter().any(|sp| sp.closed);
    let path_data = vec_path.to_svg_d();

    let obj = project
        .find_object_mut(input.object_id)
        .ok_or_else(|| ServiceError::not_found("Object not found"))?;
    obj.data = ObjectData::VectorPath {
        path_data,
        closed,
        ruler_guide_axis: None,
    };
    obj.bounds = result_bounds;
    obj.tabs.clear();
    let result = obj.clone();
    project.dirty = true;

    drop(guard);
    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.path.scaled",
        json!({
            "object": events::object_summary(&result),
        }),
    );
    Ok(result)
}

pub fn normalize_for_planner(
    ctx: &ServiceContext,
    input: NormalizeForPlannerInput,
) -> ServiceResult<Vec<NormalizedVector>> {
    let guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let mut results = Vec::new();
    for object_id in input.object_ids {
        let obj = project
            .find_object(object_id)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        // Resolve VirtualClone to its source geometry (transient, no mutation)
        let resolved = project.resolve_clone(obj);
        let effective = resolved.as_ref().unwrap_or(obj);
        if let Some(normalized) = normalize_object(effective) {
            results.push(normalized);
        }
    }

    Ok(results)
}

// ────────────────────────────────────────────────────────────
// Trim result type (shared across service + Tauri layers)
// ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimShapeResult {
    pub objects: Vec<ProjectObject>,
    /// True when healing was attempted but ambiguous.
    pub heal_failed: bool,
    /// True when healing merged pieces into an open (not fill-ready) path.
    pub open_result: bool,
}

/// Preview data for the trim overlay.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrimPreview {
    pub segment_points: Vec<[f64; 2]>,
}

// ────────────────────────────────────────────────────────────
// Shared trim target selection
// ────────────────────────────────────────────────────────────

struct TrimCandidate {
    obj_id: ObjectId,
    world_path: VecPath,
    polylines: Vec<(Vec<Point2D>, bool)>,
}

struct TrimTarget {
    source_object: ProjectObject,
    subpath_idx: usize,
    world_path: VecPath,
    cutters: Vec<(Vec<Point2D>, bool)>,
}

/// Find the best trim target for a click point.
/// Shared by both preview_trim() and trim_at_intersection().
fn find_trim_target(
    ctx: &ServiceContext,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
) -> ServiceResult<TrimTarget> {
    let guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_ref()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let click_pt = Point2D::new(click_x, click_y);

    let mut candidates: Vec<TrimCandidate> = Vec::new();

    for obj in &project.objects {
        if !obj.visible || obj.locked {
            continue;
        }
        match &obj.data {
            ObjectData::Text { .. } | ObjectData::RasterImage { .. } | ObjectData::Group { .. } => {
                continue;
            }
            _ => {}
        }

        // Resolve VirtualClone to its source geometry (transient, no mutation)
        let resolved = project.resolve_clone(obj);
        let effective = resolved.as_ref().unwrap_or(obj);

        // Exclude clone-backed text/raster/groups just like concrete
        // ones. A VirtualClone of text would otherwise pass the
        // wrapper-level filter above, get resolved to text path
        // geometry, and be treated as trimmable — which then
        // replaces the clone with plain VectorPath objects and
        // destroys the text clone's semantics.
        if matches!(
            &effective.data,
            ObjectData::Text { .. } | ObjectData::RasterImage { .. } | ObjectData::Group { .. }
        ) {
            continue;
        }

        if let Some(world_path) = object_to_world_vecpath(effective) {
            let flat = flatten_vecpath(&world_path, DEFAULT_TOLERANCE_MM);
            let polylines: Vec<(Vec<Point2D>, bool)> =
                flat.into_iter().map(|p| (p.points, p.closed)).collect();
            candidates.push(TrimCandidate {
                obj_id: obj.id,
                world_path,
                polylines,
            });
        }
    }

    if candidates.is_empty() {
        return Err(ServiceError::not_found(
            "No trimmable objects in the project",
        ));
    }

    struct EdgeHit {
        candidate_idx: usize,
        subpath_idx: usize,
        distance: f64,
    }

    let mut hits: Vec<EdgeHit> = Vec::new();
    for (ci, cand) in candidates.iter().enumerate() {
        for (si, (pts, closed)) in cand.polylines.iter().enumerate() {
            if let Some((_sp_idx, dist, _arc)) =
                trim_core::nearest_edge(click_pt, &[(pts.clone(), *closed)])
                && dist <= edge_threshold_mm
            {
                hits.push(EdgeHit {
                    candidate_idx: ci,
                    subpath_idx: si,
                    distance: dist,
                });
            }
        }
    }

    hits.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());

    if hits.is_empty() {
        return Err(ServiceError::not_found(
            "No path edges found near click point",
        ));
    }

    // Find first candidate with intersections
    for hit in &hits {
        let target_cand = &candidates[hit.candidate_idx];

        let mut cutters: Vec<(Vec<Point2D>, bool)> = Vec::new();
        for (ci, cand) in candidates.iter().enumerate() {
            if ci == hit.candidate_idx {
                for (si, poly) in cand.polylines.iter().enumerate() {
                    if si != hit.subpath_idx {
                        cutters.push(poly.clone());
                    }
                }
            } else {
                for poly in &cand.polylines {
                    cutters.push(poly.clone());
                }
            }
        }

        // Check if there are intersections (read-only check)
        if let Some(sp) = target_cand.world_path.subpaths.get(hit.subpath_idx)
            && trim_core::find_trim_bracket(sp, &cutters, click_pt).is_some()
        {
            let source_object = project
                .find_object(target_cand.obj_id)
                .ok_or_else(|| ServiceError::not_found("Source object disappeared"))?
                .clone();

            return Ok(TrimTarget {
                source_object,
                subpath_idx: hit.subpath_idx,
                world_path: target_cand.world_path.clone(),
                cutters,
            });
        }
    }

    Err(ServiceError::not_found(
        "No intersections found near click point",
    ))
}

/// Preview the trim segment (read-only).
pub fn preview_trim(
    ctx: &ServiceContext,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
) -> ServiceResult<Option<TrimPreview>> {
    let target = match find_trim_target(ctx, click_x, click_y, edge_threshold_mm) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let click_pt = Point2D::new(click_x, click_y);
    let result = trim_core::preview_trim_segment(
        &target.world_path,
        target.subpath_idx,
        &target.cutters,
        click_pt,
    );

    Ok(result.map(|pts| TrimPreview {
        segment_points: pts.iter().map(|p| [p.x, p.y]).collect(),
    }))
}

/// Trim a path at intersections with other paths.
/// The backend picks the target object by edge proximity to the click point.
pub fn trim_at_intersection(
    ctx: &ServiceContext,
    click_x: f64,
    click_y: f64,
    edge_threshold_mm: f64,
    heal: bool,
) -> ServiceResult<TrimShapeResult> {
    let target = find_trim_target(ctx, click_x, click_y, edge_threshold_mm)?;

    let click_pt = Point2D::new(click_x, click_y);
    let trim_result = trim_core::trim_at_intersection(
        &target.world_path,
        target.subpath_idx,
        &target.cutters,
        click_pt,
    );

    if trim_result.pieces.is_empty() {
        return Err(ServiceError::not_found(
            "No intersections found near click point",
        ));
    }

    // Attempt healing if requested
    let (final_paths, heal_failed, open_result) = if heal {
        match trim_core::heal_trim_results(&trim_result.pieces, &trim_result.cut_points, 0.1) {
            trim_core::HealOutcome::HealedClosed(h) => (vec![h], false, false),
            trim_core::HealOutcome::HealedOpen(h) => {
                // Gate: only attempt cutter closure if source subpath was closed
                let source_was_closed = target
                    .world_path
                    .subpaths
                    .get(target.subpath_idx)
                    .is_some_and(|sp| sp.closed);

                if source_was_closed {
                    if let (Some(left_hit), Some(right_hit)) =
                        (&trim_result.left_hit, &trim_result.right_hit)
                    {
                        let main_sp = h.subpaths.iter().find(|sp| !sp.closed);
                        if let Some(main) = main_sp {
                            if let Some(closed_sp) = trim_core::close_with_cutter_boundary(
                                main,
                                &target.cutters,
                                left_hit,
                                right_hit,
                                click_pt,
                            ) {
                                let mut result = VecPath::new();
                                result.subpaths.push(closed_sp);
                                for sp in &h.subpaths {
                                    if sp.closed {
                                        result.subpaths.push(sp.clone());
                                    }
                                }
                                (vec![result], false, false)
                            } else {
                                (vec![h], false, true)
                            }
                        } else {
                            (vec![h], false, true)
                        }
                    } else {
                        // Open-path trims or endpoint-bracket cases have None hits
                        (vec![h], false, true)
                    }
                } else {
                    (vec![h], false, true)
                }
            }
            trim_core::HealOutcome::Ambiguous(raw) => (raw, true, false),
        }
    } else {
        (trim_result.pieces, false, false)
    };

    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    let source = &target.source_object;
    let source_id = source.id;

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;
    // Auto-unlink VirtualClone before removal (preserves clone state in undo)
    project
        .ensure_resolved(source_id)
        .map_err(ServiceError::internal)?;

    project.remove_object(source_id);

    let mut created: Vec<ProjectObject> = Vec::new();
    for (i, vp) in final_paths.iter().enumerate() {
        let path_data = vp.to_svg_d();
        let closed = if open_result {
            // HealedOpen means the primary chain is open — don't let closed
            // sibling subpaths promote the object to closed.
            false
        } else {
            vp.subpaths.iter().any(|sp| sp.closed)
        };
        let bounds = vp.visual_bounds().unwrap_or(source.bounds);

        let mut new_obj = ProjectObject::new(
            format!("{} trim {}", source.name, i + 1),
            source.layer_id,
            bounds,
            ObjectData::VectorPath {
                path_data,
                closed,
                ruler_guide_axis: None,
            },
        );
        new_obj.visible = source.visible;
        new_obj.locked = source.locked;
        new_obj.z_index = source.z_index;
        new_obj.lock_aspect_ratio = source.lock_aspect_ratio;
        new_obj.power_scale = source.power_scale;
        new_obj.priority = source.priority;

        created.push(new_obj.clone());
        project.add_object(new_obj);
    }

    project.dirty = true;
    drop(guard);

    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.trim",
        json!({
            "source_object_id": source_id,
            "created_count": created.len(),
            "heal_failed": heal_failed,
        }),
    );

    Ok(TrimShapeResult {
        objects: created,
        heal_failed,
        open_result,
    })
}

/// Close and join selected objects into a single object.
pub fn close_and_join(
    ctx: &ServiceContext,
    object_ids: Vec<ObjectId>,
    tolerance: f64,
) -> ServiceResult<(ProjectObject, bool)> {
    let mut guard = ctx.project.lock().map_err(|e| lock_err("project", e))?;
    let project = guard
        .as_mut()
        .ok_or_else(|| ServiceError::not_found("No project open"))?;

    if object_ids.is_empty() {
        return Err(ServiceError::invalid_input("No objects provided"));
    }

    ctx.push_project_undo_snapshot(project)
        .map_err(ServiceError::internal)?;

    for &oid in &object_ids {
        project
            .ensure_resolved(oid)
            .map_err(ServiceError::internal)?;
    }

    let mut world_paths: Vec<VecPath> = Vec::new();
    let mut first_obj: Option<ProjectObject> = None;
    let mut max_z_index = 0i32;

    for &oid in &object_ids {
        let obj = project
            .find_object(oid)
            .ok_or_else(|| ServiceError::not_found("Object not found"))?;
        if first_obj.is_none() {
            first_obj = Some(obj.clone());
        }
        max_z_index = max_z_index.max(obj.z_index);
        let wp = object_to_world_vecpath(obj)
            .ok_or_else(|| ServiceError::invalid_input("Object is not a vector type"))?;
        world_paths.push(wp);
    }

    let first = first_obj.unwrap();
    let result = path_ops_core::close_and_join(&world_paths, tolerance);

    // Remove originals
    for &oid in &object_ids {
        project.remove_object(oid);
    }

    let path_data = result.path.to_svg_d();
    let closed = result.path.subpaths.iter().any(|sp| sp.closed);
    let bounds = result.path.visual_bounds().unwrap_or(first.bounds);

    let dest_layer = reroute_vector_result_layer(project, first.layer_id)?;
    let mut new_obj = ProjectObject::new(
        "Joined",
        dest_layer,
        bounds,
        ObjectData::VectorPath {
            path_data,
            closed,
            ruler_guide_axis: None,
        },
    );
    new_obj.visible = true;
    new_obj.locked = false;
    new_obj.z_index = max_z_index;
    new_obj.power_scale = first.power_scale;
    new_obj.priority = first.priority;
    new_obj.lock_aspect_ratio = false;

    let result_obj = new_obj.clone();
    project.add_object(new_obj);
    project.dirty = true;
    drop(guard);

    planning::invalidate_plan_cache(ctx)?;
    ctx.emit_event(
        "vector.close_and_join",
        json!({
            "source_object_ids": object_ids,
            "object": events::object_summary(&result_obj),
            "fully_closed": result.fully_closed,
        }),
    );

    Ok((result_obj, result.fully_closed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::Id;
    use beambench_core::{
        Layer, OperationType, Project, ShapeKind, TextAlignment, TextAlignmentV, TextLayoutMode,
    };

    fn sample_project() -> (ServiceContext, beambench_core::LayerId, ObjectId, ObjectId) {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Vector");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let mut rect_a = ProjectObject::new(
            "Rect A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(20.0, 30.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        rect_a.transform = Transform2D::translate(5.0, 6.0);
        let id_a = rect_a.id;

        let rect_b = ProjectObject::new(
            "Rect B",
            layer_id,
            Bounds::new(Point2D::new(15.0, 20.0), Point2D::new(25.0, 30.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let id_b = rect_b.id;

        project.add_object(rect_a);
        project.add_object(rect_b);
        *ctx.project.lock().unwrap() = Some(project);
        (ctx, layer_id, id_a, id_b)
    }

    fn rect_object(
        name: &str,
        layer_id: beambench_core::LayerId,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    ) -> ProjectObject {
        ProjectObject::new(
            name,
            layer_id,
            Bounds::new(Point2D::new(x, y), Point2D::new(x + width, y + height)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width,
                height,
                corner_radius: 0.0,
            },
        )
    }

    fn assert_next_event_type(rx: &mut tokio::sync::broadcast::Receiver<String>, expected: &str) {
        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], expected);
    }

    fn assert_binary_boolean_event(
        op: fn(&ServiceContext, BooleanOpInput) -> ServiceResult<ProjectObject>,
        expected: &str,
    ) {
        let (ctx, _layer_id, id_a, id_b) = sample_project();
        let mut rx = ctx.events.subscribe();

        op(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        assert_next_event_type(&mut rx, expected);
    }

    fn path_is_filled_evenodd(path: &VecPath, x: f64, y: f64) -> bool {
        path_contains_point_evenodd(path, Point2D::new(x, y))
    }

    fn result_vecpath(result: &ProjectObject) -> VecPath {
        match &result.data {
            ObjectData::VectorPath { path_data, .. } => VecPath::parse_svg_d(path_data),
            other => panic!("Expected VectorPath result, got {other:?}"),
        }
    }

    #[test]
    fn convert_to_path_bakes_world_geometry_and_resets_transform() {
        let (ctx, _layer_id, id_a, _id_b) = sample_project();

        let result = convert_to_path(&ctx, ConvertToPathInput { object_id: id_a }).unwrap();

        assert!(matches!(result.data, ObjectData::VectorPath { .. }));
        assert!(result.transform.is_identity());
        assert_eq!(result.bounds.min.x, 15.0);
        assert_eq!(result.bounds.min.y, 26.0);
    }

    #[test]
    fn convert_to_path_reroutes_vector_result_off_image_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Convert Route");
        let mut image_layer = Layer::new("C00 (Image)", OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#FF0000".into());
        let image_layer_id = image_layer.id;
        let mut line_layer = Layer::new("C00 (Line)", OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);

        let shape = ProjectObject::new(
            "Rect",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        );
        let object_id = shape.id;
        project.add_object(shape);
        *ctx.project.lock().unwrap() = Some(project);

        let result = convert_to_path(&ctx, ConvertToPathInput { object_id }).unwrap();

        assert_eq!(result.layer_id, line_layer_id);
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert_eq!(
            project.find_object(object_id).unwrap().layer_id,
            line_layer_id
        );
    }

    #[test]
    fn mesh_deform_selection_converts_shape_and_applies_corner_warp() {
        let (ctx, _layer_id, id_a, _id_b) = sample_project();

        let result = mesh_deform_selection(
            &ctx,
            MeshDeformSelectionInput {
                object_ids: vec![id_a],
                source_bounds: Bounds::new(Point2D::new(15.0, 26.0), Point2D::new(25.0, 36.0)),
                handles: vec![
                    Point2D::new(15.0, 26.0),
                    Point2D::new(35.0, 26.0),
                    Point2D::new(15.0, 36.0),
                    Point2D::new(25.0, 36.0),
                ],
                grid_size: 2,
                perspective: true,
            },
        )
        .unwrap();

        assert_eq!(result.len(), 1);
        assert!(matches!(result[0].data, ObjectData::VectorPath { .. }));
        assert!(result[0].transform.is_identity());
        assert!(result[0].bounds.max.x > 34.9);
        assert_eq!(result[0].bounds.min.x, 15.0);
    }

    #[test]
    fn boolean_union_replaces_original_objects() {
        let (ctx, layer_id, id_a, id_b) = sample_project();

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        assert_eq!(result.name, "Union");
        assert_eq!(result.layer_id, layer_id);
        let project = ctx.project.lock().unwrap();
        let project = project.as_ref().unwrap();
        assert!(project.find_object(id_a).is_none());
        assert!(project.find_object(id_b).is_none());
        assert!(project.find_object(result.id).is_some());
    }

    #[test]
    fn boolean_ops_emit_explicit_event_names() {
        assert_binary_boolean_event(boolean_union, "vector.boolean.union");
        assert_binary_boolean_event(boolean_subtract, "vector.boolean.subtract");
        assert_binary_boolean_event(boolean_intersection, "vector.boolean.intersection");
        assert_binary_boolean_event(boolean_exclude, "vector.boolean.exclude");

        let (ctx, _layer_id, id_a, id_b) = sample_project();
        let mut rx = ctx.events.subscribe();
        boolean_weld(
            &ctx,
            BooleanWeldInput {
                object_ids: vec![id_a, id_b],
            },
        )
        .unwrap();

        assert_next_event_type(&mut rx, "vector.boolean.weld");
    }

    #[test]
    fn boolean_accepts_closed_group_operand_and_deletes_group_leaves() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Group");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let child_a = rect_object("Child A", layer_id, 0.0, 0.0, 10.0, 10.0);
        let child_a_id = child_a.id;
        let child_b = rect_object("Child B", layer_id, 20.0, 0.0, 10.0, 10.0);
        let child_b_id = child_b.id;
        let group = ProjectObject::new(
            "Grouped Operand",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::Group {
                children: vec![child_a_id, child_b_id],
            },
        );
        let group_id = group.id;
        let cutter = rect_object("Cutter", layer_id, 8.0, 0.0, 14.0, 10.0);
        let cutter_id = cutter.id;
        project.add_object(child_a);
        project.add_object(child_b);
        project.add_object(group);
        project.add_object(cutter);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: group_id,
                object_id_b: cutter_id,
            },
        )
        .unwrap();

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(group_id).is_none());
        assert!(project.find_object(child_a_id).is_none());
        assert!(project.find_object(child_b_id).is_none());
        assert!(project.find_object(cutter_id).is_none());
        assert!(project.find_object(result.id).is_some());
    }

    #[test]
    fn boolean_subtract_preserves_overlap_inside_grouped_subject() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Group Overlap");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let child_a = rect_object("Child A", layer_id, 0.0, 0.0, 20.0, 20.0);
        let child_a_id = child_a.id;
        let child_b = rect_object("Child B", layer_id, 10.0, 0.0, 20.0, 20.0);
        let child_b_id = child_b.id;
        let group = ProjectObject::new(
            "Grouped Subject",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 20.0)),
            ObjectData::Group {
                children: vec![child_a_id, child_b_id],
            },
        );
        let group_id = group.id;
        let cutter = rect_object("Cutter", layer_id, 40.0, 0.0, 5.0, 5.0);
        let cutter_id = cutter.id;
        project.add_object(child_a);
        project.add_object(child_b);
        project.add_object(group);
        project.add_object(cutter);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_subtract(
            &ctx,
            BooleanOpInput {
                object_id_a: group_id,
                object_id_b: cutter_id,
            },
        )
        .unwrap();

        let result_path = result_vecpath(&result);
        assert!(
            path_is_filled_evenodd(&result_path, 15.0, 10.0),
            "Grouped sibling overlap should remain filled after a non-overlapping subtract"
        );
    }

    #[test]
    fn boolean_subtract_preserves_nested_group_hole_semantics() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Group Nest");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let outer = rect_object("Outer", layer_id, 0.0, 0.0, 30.0, 30.0);
        let outer_id = outer.id;
        let hole = rect_object("Hole", layer_id, 10.0, 10.0, 10.0, 10.0);
        let hole_id = hole.id;
        let group = ProjectObject::new(
            "Grouped Donut",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 30.0)),
            ObjectData::Group {
                children: vec![outer_id, hole_id],
            },
        );
        let group_id = group.id;
        let cutter = rect_object("Cutter", layer_id, 40.0, 0.0, 5.0, 5.0);
        let cutter_id = cutter.id;
        project.add_object(outer);
        project.add_object(hole);
        project.add_object(group);
        project.add_object(cutter);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_subtract(
            &ctx,
            BooleanOpInput {
                object_id_a: group_id,
                object_id_b: cutter_id,
            },
        )
        .unwrap();

        let result_path = result_vecpath(&result);
        assert!(
            !path_is_filled_evenodd(&result_path, 15.0, 15.0),
            "A contained child should still behave as a group hole"
        );
        assert!(
            path_is_filled_evenodd(&result_path, 5.0, 5.0),
            "The containing group child should remain filled"
        );
    }

    #[test]
    fn boolean_rejects_group_with_open_leaf_without_deleting_inputs() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Group Invalid");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let closed = rect_object("Closed Child", layer_id, 0.0, 0.0, 10.0, 10.0);
        let closed_id = closed.id;
        let open = ProjectObject::new(
            "Open Child",
            layer_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(30.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M20 0 L30 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let open_id = open.id;
        let group = ProjectObject::new(
            "Grouped Operand",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::Group {
                children: vec![closed_id, open_id],
            },
        );
        let group_id = group.id;
        let other = rect_object("Other", layer_id, 40.0, 0.0, 10.0, 10.0);
        let other_id = other.id;
        project.add_object(closed);
        project.add_object(open);
        project.add_object(group);
        project.add_object(other);
        *ctx.project.lock().unwrap() = Some(project);

        let err = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: group_id,
                object_id_b: other_id,
            },
        )
        .unwrap_err();

        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
        assert!(err.message.contains("not a closed boolean operand"));
        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.find_object(group_id).is_some());
        assert!(project.find_object(closed_id).is_some());
        assert!(project.find_object(open_id).is_some());
        assert!(project.find_object(other_id).is_some());
    }

    #[test]
    fn group_and_ungroup_participate_in_undo_history() {
        let (ctx, _layer_id, id_a, id_b) = sample_project();

        let group = group_objects(
            &ctx,
            GroupObjectsInput {
                object_ids: vec![id_a, id_b],
            },
        )
        .unwrap();
        assert!(ctx.undo_state().unwrap().can_undo);

        let children = ungroup_objects(&ctx, group.id).unwrap();
        assert_eq!(children, vec![id_a, id_b]);
        assert!(ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn auto_group_groups_children_into_smallest_closed_container() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Auto Group");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let big = ProjectObject::new(
            "Outer Big",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        );
        let big_id = big.id;
        let small = ProjectObject::new(
            "Outer Small",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 50.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 40.0,
                height: 40.0,
                corner_radius: 0.0,
            },
        );
        let small_id = small.id;
        let child = ProjectObject::new(
            "Child",
            layer_id,
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(25.0, 25.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let child_id = child.id;
        project.add_object(big);
        project.add_object(small);
        project.add_object(child);
        *ctx.project.lock().unwrap() = Some(project);

        let groups = auto_group_objects(
            &ctx,
            AutoGroupObjectsInput {
                object_ids: vec![big_id, small_id, child_id],
            },
        )
        .unwrap();

        assert_eq!(groups.len(), 2);
        let group_children: Vec<Vec<ObjectId>> = groups
            .iter()
            .map(|group| match &group.data {
                ObjectData::Group { children } => children.clone(),
                other => panic!("expected group, got {other:?}"),
            })
            .collect();
        assert!(group_children.contains(&vec![big_id, small_id]));
        assert!(group_children.contains(&vec![small_id, child_id]));
        assert!(ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn auto_group_excludes_open_locked_text_image_barcode_and_guides_as_outers() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Auto Group Exclusions");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let mut open = ProjectObject::new(
            "Open",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let open_id = open.id;
        let mut locked = ProjectObject::new(
            "Locked",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 100.0,
                height: 100.0,
                corner_radius: 0.0,
            },
        );
        locked.locked = true;
        let locked_id = locked.id;
        let text = ProjectObject::new(
            "Text",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Text {
                content: "Text".to_string(),
                font_family: "Arial".to_string(),
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
                resolved_path_data: None,
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: None,
            },
        );
        let text_id = text.id;
        let image = ProjectObject::new(
            "Image",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::RasterImage {
                asset_key: "asset".to_string(),
                original_width_px: 10,
                original_height_px: 10,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let image_id = image.id;
        let barcode = ProjectObject::new(
            "Barcode",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::Barcode {
                barcode_type: beambench_common::BarcodeType::Code128,
                data: "123".to_string(),
                width: 100.0,
                height: 100.0,
                options: Default::default(),
            },
        );
        let barcode_id = barcode.id;
        let guide = ProjectObject::new(
            "Guide",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L100 0".to_string(),
                closed: true,
                ruler_guide_axis: Some(GuideAxis::Horizontal),
            },
        );
        let guide_id = guide.id;
        let child = ProjectObject::new(
            "Child",
            layer_id,
            Bounds::new(Point2D::new(20.0, 20.0), Point2D::new(25.0, 25.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 5.0,
                height: 5.0,
                corner_radius: 0.0,
            },
        );
        let child_id = child.id;

        open.visible = true;
        for object in [open, locked, text, image, barcode, guide, child] {
            project.add_object(object);
        }
        *ctx.project.lock().unwrap() = Some(project);

        let groups = auto_group_objects(
            &ctx,
            AutoGroupObjectsInput {
                object_ids: vec![
                    open_id, locked_id, text_id, image_id, barcode_id, guide_id, child_id,
                ],
            },
        )
        .unwrap();
        assert!(groups.is_empty());
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn node_edit_operations_recompute_bounds() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Nodes");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = update_node(
            &ctx,
            UpdateNodeInput {
                object_id,
                subpath_idx: 0,
                command_idx: 2,
                x: 20.0,
                y: 15.0,
                handle_type: None,
            },
        )
        .unwrap();
        assert_eq!(updated.bounds.max.x, 20.0);
        assert_eq!(updated.bounds.max.y, 15.0);

        let inserted = insert_node(
            &ctx,
            InsertNodeInput {
                object_id,
                subpath_idx: 0,
                command_idx: 1,
                t: 0.5,
            },
        )
        .unwrap();
        assert!(matches!(inserted.data, ObjectData::VectorPath { .. }));

        let deleted = delete_node(
            &ctx,
            DeleteNodeInput {
                object_id,
                subpath_idx: 0,
                command_idx: 1,
            },
        )
        .unwrap();
        assert!(matches!(deleted.data, ObjectData::VectorPath { .. }));
    }

    #[test]
    fn update_nodes_batch_moves_multiple_targets() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Batch Nodes");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = update_nodes_batch(
            &ctx,
            UpdateNodesBatchInput {
                object_id,
                updates: vec![
                    BatchNodeUpdate {
                        node_id: NodeId {
                            subpath_idx: 0,
                            command_idx: 0,
                        },
                        x: 1.0,
                        y: 2.0,
                        handle_type: None,
                    },
                    BatchNodeUpdate {
                        node_id: NodeId {
                            subpath_idx: 0,
                            command_idx: 1,
                        },
                        x: 11.0,
                        y: 2.0,
                        handle_type: None,
                    },
                ],
            },
        )
        .unwrap();

        assert_eq!(updated.bounds.min.x, 1.0);
        assert_eq!(updated.bounds.min.y, 2.0);
        assert_eq!(updated.bounds.max.x, 11.0);
        assert_eq!(updated.bounds.max.y, 2.0);
    }

    #[test]
    fn update_nodes_batch_pushes_exactly_one_undo_snapshot() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Batch Undo");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        assert!(!ctx.undo_state().unwrap().can_undo);
        update_nodes_batch(
            &ctx,
            UpdateNodesBatchInput {
                object_id,
                updates: vec![
                    BatchNodeUpdate {
                        node_id: NodeId {
                            subpath_idx: 0,
                            command_idx: 0,
                        },
                        x: 1.0,
                        y: 1.0,
                        handle_type: None,
                    },
                    BatchNodeUpdate {
                        node_id: NodeId {
                            subpath_idx: 0,
                            command_idx: 1,
                        },
                        x: 11.0,
                        y: 1.0,
                        handle_type: None,
                    },
                    BatchNodeUpdate {
                        node_id: NodeId {
                            subpath_idx: 0,
                            command_idx: 2,
                        },
                        x: 11.0,
                        y: 11.0,
                        handle_type: None,
                    },
                ],
            },
        )
        .unwrap();
        assert!(ctx.undo_state().unwrap().can_undo);
        crate::ops::project::undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn delete_nodes_removes_multiple_nodes_with_one_undo_snapshot() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Delete Nodes Batch");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L20 0 L30 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = delete_nodes(
            &ctx,
            DeleteNodesInput {
                object_id,
                node_ids: vec![
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 1,
                    },
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 2,
                    },
                ],
            },
        )
        .unwrap();

        let editable = get_editable_path(&ctx, object_id).unwrap();
        assert_eq!(editable[0].nodes.len(), 2);
        assert_eq!(updated.bounds.min.x, 0.0);
        assert_eq!(updated.bounds.max.x, 30.0);
        assert!(ctx.undo_state().unwrap().can_undo);
        crate::ops::project::undo_project(&ctx).unwrap();
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn delete_nodes_removes_fully_selected_subpath_without_ghost_node() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Delete Nodes Subpath");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(30.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 M20 0 L30 0 L30 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = delete_nodes(
            &ctx,
            DeleteNodesInput {
                object_id,
                node_ids: vec![
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 0,
                    },
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 1,
                    },
                ],
            },
        )
        .unwrap();

        let editable = get_editable_path(&ctx, object_id).unwrap();
        assert_eq!(editable.len(), 1);
        assert_eq!(editable[0].nodes.len(), 3);
        assert_eq!(editable[0].nodes[0].position.x, 20.0);
        assert_eq!(updated.bounds.min.x, 20.0);
    }

    #[test]
    fn delete_nodes_rejects_deleting_every_node_without_writing_ghost_node() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Delete Every Node");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = delete_nodes(
            &ctx,
            DeleteNodesInput {
                object_id,
                node_ids: vec![
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 0,
                    },
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 1,
                    },
                    NodeId {
                        subpath_idx: 0,
                        command_idx: 2,
                    },
                ],
            },
        )
        .unwrap_err();

        assert!(err.to_string().contains("Cannot delete every node"));
        let editable = get_editable_path(&ctx, object_id).unwrap();
        assert_eq!(editable[0].nodes.len(), 3);
        assert!(!ctx.undo_state().unwrap().can_undo);
    }

    #[test]
    fn align_segment_to_angle_snaps_line_and_preserves_length() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Align Segment");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(8.0, 6.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L8 6".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = align_segment_to_angle(
            &ctx,
            SegmentOpInput {
                object_id,
                subpath_idx: 0,
                command_idx: 1,
            },
        )
        .unwrap();

        if let ObjectData::VectorPath { path_data, .. } = &updated.data {
            // Original line (0,0)→(8,6) has midpoint (4,3), length 10, angle ≈36.87°.
            // Snapped to 45° around the midpoint: endpoints (0.464…, -0.535…) and (7.535…, 6.535…).
            assert!(
                path_data.contains("0.464466") && path_data.contains("7.535534"),
                "expected midpoint-rotated 45-degree line, got {path_data}"
            );
        } else {
            panic!("expected vector path");
        }
        let length = ((updated.bounds.max.x - updated.bounds.min.x).powi(2)
            + (updated.bounds.max.y - updated.bounds.min.y).powi(2))
        .sqrt();
        assert!((length - 10.0).abs() < 1e-3);
    }

    #[test]
    fn align_segment_to_angle_rejects_curves() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Align Curve Reject");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 C3 0 7 10 10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = align_segment_to_angle(
            &ctx,
            SegmentOpInput {
                object_id,
                subpath_idx: 0,
                command_idx: 1,
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn extend_endpoint_to_intersection_rejects_non_endpoints() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Extend Reject");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = extend_endpoint_to_intersection(
            &ctx,
            ExtendEndpointInput {
                object_id,
                node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn join_subpaths_merges_two_open_subpaths_in_place() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Join Subpaths");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 M10 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = join_subpaths(
            &ctx,
            JoinSubpathsInput {
                object_id,
                src_node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
                dst_node_id: NodeId {
                    subpath_idx: 1,
                    command_idx: 0,
                },
            },
        )
        .unwrap();

        if let ObjectData::VectorPath { path_data, .. } = &updated.data {
            assert!(path_data.starts_with("M0 0 L10 0"));
            assert!(path_data.contains("L20 0"));
            assert_eq!(path_data.matches('M').count(), 1);
        } else {
            panic!("expected vector path");
        }
    }

    #[test]
    fn join_subpaths_rejects_self_join() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Join Reject");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 M10 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = join_subpaths(
            &ctx,
            JoinSubpathsInput {
                object_id,
                src_node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
                dst_node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn join_subpaths_rejects_non_endpoint_source() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Join Reject Mid");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L5 0 L10 0 M10 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = join_subpaths(
            &ctx,
            JoinSubpathsInput {
                object_id,
                src_node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
                dst_node_id: NodeId {
                    subpath_idx: 1,
                    command_idx: 0,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn extend_endpoint_to_intersection_rejects_closed_path() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Extend Closed Reject");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let err = extend_endpoint_to_intersection(
            &ctx,
            ExtendEndpointInput {
                object_id,
                node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 0,
                },
            },
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn extend_endpoint_to_intersection_extends_along_tangent() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Extend Tangent");
        let layer_id = project.ensure_default_layer();
        let line = ProjectObject::new(
            "Line",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let line_id = line.id;
        project.add_object(line);
        let wall = ProjectObject::new(
            "Wall",
            layer_id,
            Bounds::new(Point2D::new(20.0, -5.0), Point2D::new(20.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M20 -5 L20 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        project.add_object(wall);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = extend_endpoint_to_intersection(
            &ctx,
            ExtendEndpointInput {
                object_id: line_id,
                node_id: NodeId {
                    subpath_idx: 0,
                    command_idx: 1,
                },
            },
        )
        .unwrap();

        if let ObjectData::VectorPath { path_data, .. } = &updated.data {
            assert!(
                path_data.contains("L20 0") || path_data.contains("L 20 0"),
                "expected endpoint extended to (20,0) along tangent, got {path_data}"
            );
        } else {
            panic!("expected vector path");
        }
        assert!((updated.bounds.max.x - 20.0).abs() < 1e-3);
    }

    #[test]
    fn trim_segment_to_intersection_brackets_to_nearest_intersections() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Trim Bracket");
        let layer_id = project.ensure_default_layer();
        let target = ProjectObject::new(
            "Target",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let target_id = target.id;
        project.add_object(target);
        let cutter_a = ProjectObject::new(
            "CutterA",
            layer_id,
            Bounds::new(Point2D::new(5.0, -5.0), Point2D::new(5.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M5 -5 L5 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        project.add_object(cutter_a);
        let cutter_b = ProjectObject::new(
            "CutterB",
            layer_id,
            Bounds::new(Point2D::new(15.0, -5.0), Point2D::new(15.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M15 -5 L15 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        project.add_object(cutter_b);
        *ctx.project.lock().unwrap() = Some(project);

        let updated = trim_segment_to_intersection(
            &ctx,
            SegmentOpInput {
                object_id: target_id,
                subpath_idx: 0,
                command_idx: 1,
            },
            10.0,
            0.0,
        )
        .unwrap();

        if let ObjectData::VectorPath { path_data, .. } = &updated.data {
            // The middle segment between the two cutters at x=5 and x=15 is removed,
            // leaving the [0,5] and [15,20] fragments as separate subpaths.
            let move_count = path_data.matches('M').count();
            assert!(
                move_count >= 2,
                "expected at least 2 subpaths after bracket trim, got `{path_data}`"
            );
            assert!(path_data.contains("M0 0") || path_data.contains("M 0 0"));
            assert!(path_data.contains("L5 0") || path_data.contains("L 5 0"));
            assert!(path_data.contains("L20 0") || path_data.contains("L 20 0"));
        } else {
            panic!("expected vector path");
        }
    }

    #[test]
    fn scale_to_bounds_updates_path_data_and_bounds() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Scale");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let scaled = scale_path_to_bounds(
            &ctx,
            ScalePathToBoundsInput {
                object_id,
                new_min_x: 5.0,
                new_min_y: 6.0,
                new_max_x: 25.0,
                new_max_y: 26.0,
            },
        )
        .unwrap();

        assert_eq!(scaled.bounds.min.x, 5.0);
        assert_eq!(scaled.bounds.min.y, 6.0);
        assert_eq!(scaled.bounds.max.x, 25.0);
        assert_eq!(scaled.bounds.max.y, 26.0);
    }

    #[test]
    fn boolean_union_works_with_vector_path_objects() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("VecBool");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let path_a = ProjectObject::new(
            "Path A",
            layer_id,
            Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
            ObjectData::VectorPath {
                path_data: "M10 10 L30 10 L30 30 L10 30 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let id_a = path_a.id;

        let path_b = ProjectObject::new(
            "Path B",
            layer_id,
            Bounds::new(Point2D::new(20.0, 10.0), Point2D::new(40.0, 30.0)),
            ObjectData::VectorPath {
                path_data: "M20 10 L40 10 L40 30 L20 30 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let id_b = path_b.id;

        project.add_object(path_a);
        project.add_object(path_b);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        // Should produce a merged shape, not error
        assert!(matches!(result.data, ObjectData::VectorPath { .. }));
        if let ObjectData::VectorPath { path_data, .. } = &result.data {
            assert!(!path_data.is_empty());
        }
        // Bounds should span from ~10 to ~40 in x (the union of both rects)
        assert!(result.bounds.min.x <= 11.0);
        assert!(result.bounds.max.x >= 39.0);
    }

    #[test]
    fn boolean_union_preserves_stretched_polygon_shape() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("BoolStretch");
        let layer_id = project.ensure_default_layer();

        let stretched_hex = ProjectObject::new(
            "Hex",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(120.0, 40.0)),
            ObjectData::Polygon {
                sides: 6,
                radius: 20.0,
            },
        );
        let hex_id = stretched_hex.id;

        let rect = ProjectObject::new(
            "Rect",
            layer_id,
            Bounds::new(Point2D::new(80.0, 10.0), Point2D::new(140.0, 30.0)),
            ObjectData::Shape {
                kind: beambench_core::ShapeKind::Rectangle,
                width: 60.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        );
        let rect_id = rect.id;

        project.add_object(stretched_hex);
        project.add_object(rect);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: hex_id,
                object_id_b: rect_id,
            },
        )
        .unwrap();

        let aspect = result.bounds.width() / result.bounds.height();
        assert!(
            aspect > 2.0,
            "Expected stretched union result to remain wider than tall, got aspect ratio {aspect}"
        );
    }

    #[test]
    fn boolean_union_reroutes_result_off_image_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Route");
        let mut image_layer = Layer::new("C01 (Image)", OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#00FF00".into());
        let image_layer_id = image_layer.id;
        let mut line_layer = Layer::new("C01 (Line)", OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);

        let rect_a = ProjectObject::new(
            "A",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        );
        let id_a = rect_a.id;
        let rect_b = ProjectObject::new(
            "B",
            image_layer_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(30.0, 20.0)),
            ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: 20.0,
                height: 20.0,
                corner_radius: 0.0,
            },
        );
        let id_b = rect_b.id;
        project.add_object(rect_a);
        project.add_object(rect_b);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        assert_eq!(result.layer_id, line_layer_id);
    }

    #[test]
    fn boolean_exclude_reroutes_result_off_image_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Bool Exclude Route");
        let mut image_layer = Layer::new("C02 (Image)", OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#0080FF".into());
        let image_layer_id = image_layer.id;
        let mut line_layer = Layer::new("C02 (Line)", OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);

        let rect_a = rect_object("A", image_layer_id, 0.0, 0.0, 20.0, 20.0);
        let id_a = rect_a.id;
        let rect_b = rect_object("B", image_layer_id, 10.0, 0.0, 20.0, 20.0);
        let id_b = rect_b.id;
        project.add_object(rect_a);
        project.add_object(rect_b);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_exclude(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        assert_eq!(result.layer_id, line_layer_id);
    }

    #[test]
    fn editable_path_rejects_non_vector_objects() {
        let (ctx, _layer_id, id_a, _id_b) = sample_project();
        let err = get_editable_path(&ctx, id_a).unwrap_err();
        assert_eq!(err.code, crate::error::ServiceErrorCode::InvalidInput);
    }

    #[test]
    fn normalize_for_planner_returns_existing_vectors() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Normalize");
        let layer_id = project.ensure_default_layer();
        let object = ProjectObject::new(
            "Path",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0 L10 10 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let object_id = object.id;
        project.add_object(object);
        *ctx.project.lock().unwrap() = Some(project);

        let normalized = normalize_for_planner(
            &ctx,
            NormalizeForPlannerInput {
                object_ids: vec![object_id, Id::new()],
            },
        );

        assert!(normalized.is_err());
    }

    #[test]
    fn convert_to_path_emits_event() {
        let (ctx, _layer_id, object_id, _other_id) = sample_project();
        let mut rx = ctx.events.subscribe();

        let result = convert_to_path(&ctx, ConvertToPathInput { object_id }).unwrap();

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "vector.converted_to_path");
        assert_eq!(
            parsed["payload"]["object"]["id"],
            serde_json::json!(result.id)
        );
    }

    #[test]
    fn close_and_join_reroutes_result_off_image_layer() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Join Route");
        let mut image_layer = Layer::new("C02 (Image)", OperationType::Image);
        image_layer.color_tag = beambench_common::ColorTag("#0000FF".into());
        let image_layer_id = image_layer.id;
        let mut line_layer = Layer::new("C02 (Line)", OperationType::Line);
        line_layer.color_tag = image_layer.color_tag.clone();
        let line_layer_id = line_layer.id;
        project.add_layer(image_layer);
        project.add_layer(line_layer);

        let path_a = ProjectObject::new(
            "A",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M0 0 L10 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let id_a = path_a.id;
        let path_b = ProjectObject::new(
            "B",
            image_layer_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(20.0, 0.0)),
            ObjectData::VectorPath {
                path_data: "M10 0 L20 0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let id_b = path_b.id;
        project.add_object(path_a);
        project.add_object(path_b);
        *ctx.project.lock().unwrap() = Some(project);

        let (result, _) = close_and_join(&ctx, vec![id_a, id_b], 0.1).unwrap();

        assert_eq!(result.layer_id, line_layer_id);
    }

    // ── Trim service-layer tests ──

    /// Helper: create a project with two overlapping horizontal lines for trim tests.
    /// Line A: (0,5)→(20,5), Line B: (10,0)→(10,10) — they cross at (10,5).
    fn trim_project() -> (ServiceContext, beambench_core::LayerId) {
        let ctx = ServiceContext::new();
        let mut project = Project::new("Trim");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let line_a = ProjectObject::new(
            "Horiz",
            layer_id,
            Bounds::new(Point2D::new(0.0, 5.0), Point2D::new(20.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M0 5 L20 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let line_b = ProjectObject::new(
            "Vert",
            layer_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        project.add_object(line_a);
        project.add_object(line_b);
        *ctx.project.lock().unwrap() = Some(project);
        (ctx, layer_id)
    }

    #[test]
    fn trim_preserves_metadata() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimMeta");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let mut line_a = ProjectObject::new(
            "Horiz",
            layer_id,
            Bounds::new(Point2D::new(0.0, 5.0), Point2D::new(20.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M0 5 L20 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        line_a.power_scale = 0.75;
        line_a.priority = 42;
        line_a.lock_aspect_ratio = true;

        let line_b = ProjectObject::new(
            "Vert",
            layer_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        project.add_object(line_a);
        project.add_object(line_b);
        *ctx.project.lock().unwrap() = Some(project);

        // Click on the right half of horizontal line (near 15,5)
        let result = trim_at_intersection(&ctx, 15.0, 5.0, 2.0, false).unwrap();

        assert!(
            !result.objects.is_empty(),
            "Trim should produce at least one new object"
        );
        for obj in &result.objects {
            assert_eq!(obj.power_scale, 0.75, "power_scale should be inherited");
            assert_eq!(obj.priority, 42, "priority should be inherited");
            assert!(
                obj.lock_aspect_ratio,
                "lock_aspect_ratio should be inherited"
            );
            assert_eq!(obj.layer_id, layer_id, "layer_id should be inherited");
        }
    }

    #[test]
    fn trim_skips_text_objects() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimText");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        // Only a text object near the click — should not be trimmable
        let text_obj = ProjectObject::new(
            "Label",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::default(),
                alignment_v: TextAlignmentV::default(),
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
                variable_text: None,
            },
        );
        project.add_object(text_obj);
        *ctx.project.lock().unwrap() = Some(project);

        let result = trim_at_intersection(&ctx, 10.0, 5.0, 5.0, true);
        assert!(result.is_err(), "Should error — no trimmable objects");
    }

    /// Regression: a VirtualClone whose source is text must be
    /// rejected by the trim tool just like concrete text. The
    /// previous wrapper-level-only filter let clone-backed text slip
    /// through, resolve to text path geometry, and get trimmed as
    /// plain vector paths — which destroyed the clone's semantics.
    #[test]
    fn trim_skips_virtual_clone_of_text() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimCloneText");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let text_obj = ProjectObject::new(
            "Label",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(20.0, 10.0)),
            ObjectData::Text {
                content: "Hello".to_string(),
                font_family: "Arial".to_string(),
                font_size_mm: 5.0,
                alignment: TextAlignment::default(),
                alignment_v: TextAlignmentV::default(),
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
                variable_text: None,
            },
        );
        let text_id = text_obj.id;
        project.add_object(text_obj);

        // VirtualClone positioned near the intended click point. The
        // clone's bounds determine the test hit area; its source is
        // the text above, which should protect the clone from trim.
        let clone = ProjectObject::new(
            "Clone",
            layer_id,
            Bounds::new(Point2D::new(30.0, 0.0), Point2D::new(50.0, 10.0)),
            ObjectData::VirtualClone { source_id: text_id },
        );
        project.add_object(clone);
        *ctx.project.lock().unwrap() = Some(project);

        // Click squarely on the clone's bounds — if the filter let
        // it through, we'd get a trim result; if the filter is
        // symmetric with the concrete-text case we get an error.
        let result = trim_at_intersection(&ctx, 40.0, 5.0, 5.0, true);
        assert!(
            result.is_err(),
            "VirtualClone of text must not be trimmable"
        );
    }

    #[test]
    fn trim_skips_locked_objects() {
        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimLocked");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        let mut line = ProjectObject::new(
            "Locked",
            layer_id,
            Bounds::new(Point2D::new(0.0, 5.0), Point2D::new(20.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M0 5 L20 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        line.locked = true;

        project.add_object(line);
        *ctx.project.lock().unwrap() = Some(project);

        let result = trim_at_intersection(&ctx, 10.0, 5.0, 5.0, true);
        assert!(result.is_err(), "Should error — locked objects are skipped");
    }

    #[test]
    fn trim_nearest_successful_candidate() {
        // Two lines: one alone (no intersections), one crossed
        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimNearest");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        // Lone line at y=3 (close to click at y=3, no intersections)
        let lone = ProjectObject::new(
            "Lone",
            layer_id,
            Bounds::new(Point2D::new(0.0, 3.0), Point2D::new(20.0, 3.0)),
            ObjectData::VectorPath {
                path_data: "M0 3 L20 3".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        // Crossed lines at y=5 — farther from click, but has intersection
        let horiz = ProjectObject::new(
            "Horiz",
            layer_id,
            Bounds::new(Point2D::new(0.0, 5.0), Point2D::new(20.0, 5.0)),
            ObjectData::VectorPath {
                path_data: "M0 5 L20 5".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let vert = ProjectObject::new(
            "Vert",
            layer_id,
            Bounds::new(Point2D::new(10.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M10 0 L10 10".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        project.add_object(lone);
        project.add_object(horiz);
        project.add_object(vert);
        *ctx.project.lock().unwrap() = Some(project);

        // Click near y=4 — lone line is nearest but has no intersections,
        // second-nearest (horiz at y=5) does cross vert.
        let result = trim_at_intersection(&ctx, 15.0, 4.0, 5.0, true);
        assert!(
            result.is_ok(),
            "Should succeed by trying second-nearest candidate: {:?}",
            result.err()
        );
        let created = result.unwrap();
        assert!(
            !created.objects.is_empty(),
            "Should produce trimmed pieces from the crossed line"
        );
    }

    #[test]
    fn trim_emits_event_and_sets_dirty() {
        let (ctx, _layer_id) = trim_project();
        let mut rx = ctx.events.subscribe();

        let _result = trim_at_intersection(&ctx, 15.0, 5.0, 2.0, true).unwrap();

        let msg = rx.try_recv().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        assert_eq!(parsed["type"], "vector.trim");
        assert!(parsed["payload"]["created_count"].as_u64().unwrap() >= 1);

        let guard = ctx.project.lock().unwrap();
        let project = guard.as_ref().unwrap();
        assert!(project.dirty, "Project should be dirty after trim");
    }

    #[test]
    fn trim_supports_undo() {
        let (ctx, _layer_id) = trim_project();

        let guard = ctx.project.lock().unwrap();
        let _obj_count_before = guard.as_ref().unwrap().objects.len();
        drop(guard);

        let _result = trim_at_intersection(&ctx, 15.0, 5.0, 2.0, true).unwrap();

        assert!(ctx.undo_state().unwrap().can_undo, "Should be undoable");
    }

    #[test]
    fn trim_closed_circles_produces_closed_result() {
        use beambench_core::vector::convert::shape_to_vecpath;
        use beambench_core::vector::transform::bake_transform;

        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimCircles");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        // Circle A: r=20 (bounds 0,0 - 40,40), center (20,20)
        let vp_a = shape_to_vecpath(ShapeKind::Ellipse, 40.0, 40.0, 0.0);
        let circle_a = ProjectObject::new(
            "Circle A",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
            ObjectData::VectorPath {
                path_data: vp_a.to_svg_d(),
                closed: true,
                ruler_guide_axis: None,
            },
        );

        // Circle B: same size, shifted 20mm right → center (40,20)
        let vp_b = bake_transform(
            &shape_to_vecpath(ShapeKind::Ellipse, 40.0, 40.0, 0.0),
            &Transform2D {
                a: 1.0,
                b: 0.0,
                c: 0.0,
                d: 1.0,
                tx: 20.0,
                ty: 0.0,
            },
        );
        let circle_b = ProjectObject::new(
            "Circle B",
            layer_id,
            Bounds::new(Point2D::new(20.0, 0.0), Point2D::new(60.0, 40.0)),
            ObjectData::VectorPath {
                path_data: vp_b.to_svg_d(),
                closed: true,
                ruler_guide_axis: None,
            },
        );

        project.add_object(circle_a);
        project.add_object(circle_b);
        *ctx.project.lock().unwrap() = Some(project);

        // Click on circle A's edge inside the overlap region.
        // Circle A center=(20,20) r=20. At angle 0° the edge is (40,20).
        // Click at (39, 20) — 1mm inside the edge, within the overlap zone.
        let result = trim_at_intersection(&ctx, 39.0, 20.0, 2.0, true).unwrap();

        // Cutter boundary closure should produce a closed, fill-ready result
        assert!(
            !result.open_result,
            "Trimming closed circle against closed cutter should produce closed result (open_result=false)"
        );
        assert!(!result.heal_failed, "Healing should not fail");
        assert_eq!(result.objects.len(), 1, "Should produce exactly one object");

        // The created object's VectorPath should be closed
        let path_data_str = match &result.objects[0].data {
            ObjectData::VectorPath {
                closed, path_data, ..
            } => {
                assert!(closed, "Created object should have closed=true");
                path_data.clone()
            }
            other => panic!("Expected VectorPath, got {:?}", other),
        };

        // Verify the click-outside-contour rule: click (39,20) must be outside
        // the resulting crescent (the overlap arc was removed, so the kept shape
        // is the part of circle A that does NOT overlap with circle B).
        use beambench_common::path::VecPath;
        let result_path = VecPath::parse_svg_d(&path_data_str);
        let polys = flatten_vecpath(&result_path, DEFAULT_TOLERANCE_MM);
        assert!(
            !polys.is_empty(),
            "Result should have at least one polyline"
        );
        let pts = &polys[0].points;
        // Inline ray-casting point-in-polygon
        let (px, py) = (39.0, 20.0);
        let n = pts.len();
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let yi = pts[i].y;
            let yj = pts[j].y;
            if ((yi > py) != (yj > py))
                && (px < (pts[j].x - pts[i].x) * (py - yi) / (yj - yi) + pts[i].x)
            {
                inside = !inside;
            }
            j = i;
        }
        assert!(
            !inside,
            "Click point (39,20) must be outside the kept contour (it was in the removed overlap region)"
        );
    }

    #[test]
    fn service_ambiguous_closure_stays_open() {
        // Trim a closed circle against TWO different cutters that each cross it.
        // The trim bracket intersections come from different cutters, so
        // close_with_cutter_boundary should reject and produce open_result = true.
        use beambench_core::vector::convert::shape_to_vecpath;

        let ctx = ServiceContext::new();
        let mut project = Project::new("TrimAmbiguous");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        // Circle: r=20 (bounds 0,0 - 40,40), center (20,20)
        let vp_circle = shape_to_vecpath(ShapeKind::Ellipse, 40.0, 40.0, 0.0);
        let circle = ProjectObject::new(
            "Circle",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(40.0, 40.0)),
            ObjectData::VectorPath {
                path_data: vp_circle.to_svg_d(),
                closed: true,
                ruler_guide_axis: None,
            },
        );

        // Cutter A: vertical line through the left side at x=10
        let cutter_a = ProjectObject::new(
            "CutterA",
            layer_id,
            Bounds::new(Point2D::new(10.0, -10.0), Point2D::new(10.0, 50.0)),
            ObjectData::VectorPath {
                path_data: "M10 -10 L10 50".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        // Cutter B: vertical line through the right side at x=30
        let cutter_b = ProjectObject::new(
            "CutterB",
            layer_id,
            Bounds::new(Point2D::new(30.0, -10.0), Point2D::new(30.0, 50.0)),
            ObjectData::VectorPath {
                path_data: "M30 -10 L30 50".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );

        project.add_object(circle);
        project.add_object(cutter_a);
        project.add_object(cutter_b);
        *ctx.project.lock().unwrap() = Some(project);

        // Click on the top arc between x=10 and x=30 (inside the circle)
        let result = trim_at_intersection(&ctx, 20.0, 2.0, 5.0, true).unwrap();

        // Since the bracket intersections come from different cutters (A and B),
        // the closure gate should reject and produce open_result = true.
        assert!(
            result.open_result,
            "Trimming between two different cutters should stay open (open_result=true)"
        );
    }

    #[test]
    fn boolean_union_result_object_uses_visual_bounds() {
        // Two overlapping VectorPath objects with cubics (extended handles)
        let ctx = ServiceContext::new();
        let mut project = Project::new("BoolBounds");
        let layer = Layer::new("Work", OperationType::Cut);
        let layer_id = layer.id;
        project.add_layer(layer);

        // Shape A: cubic with control points at y=500 (visual peak ≈375)
        let path_a = "M0 0 C0 500 100 500 100 0 Z";
        let vp_a = VecPath::parse_svg_d(path_a);
        let vis_a = vp_a.visual_bounds().unwrap();
        let obj_a = ProjectObject::new(
            "A",
            layer_id,
            vis_a,
            ObjectData::VectorPath {
                path_data: path_a.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let id_a = obj_a.id;

        // Shape B: overlapping, also with cubics
        let path_b = "M50 0 C50 500 150 500 150 0 Z";
        let vp_b = VecPath::parse_svg_d(path_b);
        let vis_b = vp_b.visual_bounds().unwrap();
        let obj_b = ProjectObject::new(
            "B",
            layer_id,
            vis_b,
            ObjectData::VectorPath {
                path_data: path_b.to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let id_b = obj_b.id;

        project.add_object(obj_a);
        project.add_object(obj_b);
        *ctx.project.lock().unwrap() = Some(project);

        let result = boolean_union(
            &ctx,
            BooleanOpInput {
                object_id_a: id_a,
                object_id_b: id_b,
            },
        )
        .unwrap();

        // The returned object's bounds must be visual (curve-sampled), not hull
        // Hull would give max.y ≈ 500 (control point); visual should be < 400
        assert!(
            result.bounds.max.y < 400.0,
            "boolean result bounds.max.y={} should be visual (< 400), not hull (500)",
            result.bounds.max.y
        );
        assert!(
            result.bounds.max.y > 300.0,
            "curve does bulge significantly, max.y={}",
            result.bounds.max.y
        );

        // Cross-check: bounds match visual_bounds of stored path_data
        if let ObjectData::VectorPath { ref path_data, .. } = result.data {
            let stored_vp = VecPath::parse_svg_d(path_data);
            let stored_vis = stored_vp.visual_bounds().unwrap();
            assert!(
                (result.bounds.min.x - stored_vis.min.x).abs() < 1.0,
                "min.x mismatch: {} vs {}",
                result.bounds.min.x,
                stored_vis.min.x
            );
            assert!(
                (result.bounds.min.y - stored_vis.min.y).abs() < 1.0,
                "min.y mismatch: {} vs {}",
                result.bounds.min.y,
                stored_vis.min.y
            );
            assert!(
                (result.bounds.max.x - stored_vis.max.x).abs() < 1.0,
                "max.x mismatch: {} vs {}",
                result.bounds.max.x,
                stored_vis.max.x
            );
            assert!(
                (result.bounds.max.y - stored_vis.max.y).abs() < 1.0,
                "max.y mismatch: {} vs {}",
                result.bounds.max.y,
                stored_vis.max.y
            );
        } else {
            panic!("expected VectorPath");
        }
    }
}
