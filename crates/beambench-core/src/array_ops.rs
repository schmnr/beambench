use std::f64::consts::PI;

use beambench_common::{Bounds, Point2D, Transform2D, VecPath};
use serde::{Deserialize, Serialize};

use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::project::Project;
use crate::variable_text::{self, VariableTextConfig, VariableTextMode};
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};

/// How spacing values are interpreted.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpacingMode {
    /// Spacing is center-to-center (default).
    #[default]
    CenterToCenter,
    /// Spacing is gap between edges.
    EdgeToEdge,
}

/// How a grid axis is sized.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GridArraySizingMode {
    /// Axis is driven directly by row/column count.
    #[default]
    Count,
    /// Axis count is fitted to a requested total extent.
    Total,
}

/// Configuration for grid array operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridArrayConfig {
    pub rows: u32,
    pub cols: u32,
    #[serde(default, alias = "sizing_mode_x")]
    pub sizing_mode_x: GridArraySizingMode,
    #[serde(default, alias = "sizing_mode_y")]
    pub sizing_mode_y: GridArraySizingMode,
    #[serde(default, alias = "total_width_mm")]
    pub total_width_mm: Option<f64>,
    #[serde(default, alias = "total_height_mm")]
    pub total_height_mm: Option<f64>,
    #[serde(alias = "h_spacing_mm")]
    pub h_spacing_mm: f64,
    #[serde(alias = "v_spacing_mm")]
    pub v_spacing_mm: f64,
    #[serde(default, alias = "spacing_mode")]
    pub spacing_mode: SpacingMode,
    #[serde(default, alias = "mirror_alternate_cols")]
    pub mirror_alternate_cols: bool,
    #[serde(default, alias = "mirror_alternate_rows")]
    pub mirror_alternate_rows: bool,
    #[serde(default, alias = "x_col_shift_mm")]
    pub x_col_shift_mm: f64,
    #[serde(default, alias = "y_row_shift_mm")]
    pub y_row_shift_mm: f64,
    #[serde(default, alias = "half_shift")]
    pub half_shift: bool,
    #[serde(default, alias = "reverse_h")]
    pub reverse_h: bool,
    #[serde(default, alias = "reverse_v")]
    pub reverse_v: bool,
    #[serde(default, alias = "random_orientation")]
    pub random_orientation: bool,
    #[serde(default, alias = "random_seed")]
    pub random_seed: u64,
    /// Group all copies into one Group object (consumed by command layer).
    #[serde(default, alias = "group_results")]
    pub group_results: bool,
    /// Create VirtualClone copies instead of real copies (consumed by command layer).
    #[serde(default, alias = "create_virtual")]
    pub create_virtual: bool,
    /// Auto-increment variable text for Text objects with variable_text config.
    #[serde(default, alias = "auto_increment_text")]
    pub auto_increment_text: bool,
    /// Increment value for auto-increment text (default 1).
    #[serde(default = "default_text_increment", alias = "text_increment")]
    pub text_increment: i64,
}

fn default_text_increment() -> i64 {
    1
}

fn effective_grid_spacing(
    sources: &[ProjectObject],
    config: &GridArrayConfig,
) -> Option<(f64, f64)> {
    if sources.is_empty() {
        return None;
    }
    let cb = combined_bounds(sources);
    let combined_width = cb.width();
    let combined_height = cb.height();
    let eff_h = match config.spacing_mode {
        SpacingMode::CenterToCenter => config.h_spacing_mm,
        SpacingMode::EdgeToEdge => config.h_spacing_mm + combined_width,
    };
    let eff_v = match config.spacing_mode {
        SpacingMode::CenterToCenter => config.v_spacing_mm,
        SpacingMode::EdgeToEdge => config.v_spacing_mm + combined_height,
    };
    Some((eff_h, eff_v))
}

fn grid_cell_translation(
    config: &GridArrayConfig,
    eff_h: f64,
    eff_v: f64,
    row: u32,
    col: u32,
) -> (f64, f64) {
    let effective_col = if config.reverse_h {
        -(col as f64)
    } else {
        col as f64
    };
    let effective_row = if config.reverse_v {
        -(row as f64)
    } else {
        row as f64
    };

    let mut tx = effective_col * eff_h + config.x_col_shift_mm * effective_row;
    let ty = effective_row * eff_v + config.y_row_shift_mm * effective_col;

    if config.half_shift && row % 2 == 1 {
        tx += eff_h / 2.0;
    }

    (tx, ty)
}

pub fn grid_array_layout_bounds(
    sources: &[ProjectObject],
    config: &GridArrayConfig,
) -> Option<Bounds> {
    if sources.is_empty() {
        return None;
    }
    let (eff_h, eff_v) = effective_grid_spacing(sources, config)?;

    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for row in 0..config.rows.max(1) {
        for col in 0..config.cols.max(1) {
            let (tx, ty) = grid_cell_translation(config, eff_h, eff_v, row, col);
            for source in sources {
                min_x = min_x.min(source.bounds.min.x + tx);
                min_y = min_y.min(source.bounds.min.y + ty);
                max_x = max_x.max(source.bounds.max.x + tx);
                max_y = max_y.max(source.bounds.max.y + ty);
            }
        }
    }

    Some(Bounds::new(
        Point2D::new(min_x, min_y),
        Point2D::new(max_x, max_y),
    ))
}

fn layout_fits_total(bounds: Bounds, config: &GridArrayConfig) -> bool {
    const EPSILON: f64 = 1e-9;

    if matches!(config.sizing_mode_x, GridArraySizingMode::Total)
        && bounds.width() - config.total_width_mm.unwrap_or(0.0) > EPSILON
    {
        return false;
    }
    if matches!(config.sizing_mode_y, GridArraySizingMode::Total)
        && bounds.height() - config.total_height_mm.unwrap_or(0.0) > EPSILON
    {
        return false;
    }
    true
}

fn leftover_extent(bounds: Bounds, config: &GridArrayConfig) -> f64 {
    let mut leftover = 0.0;
    if matches!(config.sizing_mode_x, GridArraySizingMode::Total) {
        leftover += (config.total_width_mm.unwrap_or(bounds.width()) - bounds.width()).max(0.0);
    }
    if matches!(config.sizing_mode_y, GridArraySizingMode::Total) {
        leftover += (config.total_height_mm.unwrap_or(bounds.height()) - bounds.height()).max(0.0);
    }
    leftover
}

pub fn fit_grid_array_counts(
    sources: &[ProjectObject],
    config: &GridArrayConfig,
    max_axis_count: u32,
) -> (u32, u32) {
    if sources.is_empty() {
        return (config.rows.max(1), config.cols.max(1));
    }
    if matches!(config.sizing_mode_x, GridArraySizingMode::Count)
        && matches!(config.sizing_mode_y, GridArraySizingMode::Count)
    {
        return (config.rows.max(1), config.cols.max(1));
    }

    let mut best: Option<(u32, u32, f64)> = None;
    let row_range = if matches!(config.sizing_mode_y, GridArraySizingMode::Total) {
        1..=max_axis_count.max(1)
    } else {
        config.rows.max(1)..=config.rows.max(1)
    };
    let col_range = if matches!(config.sizing_mode_x, GridArraySizingMode::Total) {
        1..=max_axis_count.max(1)
    } else {
        config.cols.max(1)..=config.cols.max(1)
    };

    for rows in row_range.clone() {
        for cols in col_range.clone() {
            let mut candidate = config.clone();
            candidate.rows = rows;
            candidate.cols = cols;
            let Some(bounds) = grid_array_layout_bounds(sources, &candidate) else {
                continue;
            };
            if !layout_fits_total(bounds, &candidate) {
                continue;
            }
            let candidate_score = (rows * cols, cols, rows, leftover_extent(bounds, &candidate));
            match best {
                None => best = Some((rows, cols, candidate_score.3)),
                Some((best_rows, best_cols, best_leftover)) => {
                    let best_score = (best_rows * best_cols, best_cols, best_rows, best_leftover);
                    if candidate_score.0 > best_score.0
                        || (candidate_score.0 == best_score.0 && candidate_score.1 > best_score.1)
                        || (candidate_score.0 == best_score.0
                            && candidate_score.1 == best_score.1
                            && candidate_score.2 > best_score.2)
                        || (candidate_score.0 == best_score.0
                            && candidate_score.1 == best_score.1
                            && candidate_score.2 == best_score.2
                            && candidate_score.3 < best_score.3)
                    {
                        best = Some((rows, cols, candidate_score.3));
                    }
                }
            }
        }
    }

    best.map(|(rows, cols, _)| (rows, cols)).unwrap_or((
        if matches!(config.sizing_mode_y, GridArraySizingMode::Total) {
            1
        } else {
            config.rows.max(1)
        },
        if matches!(config.sizing_mode_x, GridArraySizingMode::Total) {
            1
        } else {
            config.cols.max(1)
        },
    ))
}

/// Compute combined bounds of a set of source objects.
fn combined_bounds(sources: &[ProjectObject]) -> Bounds {
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for s in sources {
        min_x = min_x.min(s.bounds.min.x);
        min_y = min_y.min(s.bounds.min.y);
        max_x = max_x.max(s.bounds.max.x);
        max_y = max_y.max(s.bounds.max.y);
    }
    Bounds {
        min: Point2D { x: min_x, y: min_y },
        max: Point2D { x: max_x, y: max_y },
    }
}

/// Deterministic pseudo-random rotation: returns 0, 90, 180, or 270 degrees in radians.
fn hash_rotation(seed: u64, index: usize) -> f64 {
    let mut h = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    h = h.wrapping_add(index as u64);
    h = h
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let choice = (h >> 33) % 4;
    choice as f64 * PI / 2.0
}

/// Create a grid array of objects.
/// Takes multiple source objects and arrays them as a unit.
pub fn grid_array(sources: &[ProjectObject], config: &GridArrayConfig) -> Vec<ProjectObject> {
    grid_array_with_context(None, sources, config)
}

pub fn grid_array_in_project(
    project: &Project,
    sources: &[ProjectObject],
    config: &GridArrayConfig,
) -> Vec<ProjectObject> {
    grid_array_with_context(Some(project), sources, config)
}

fn grid_array_with_context(
    project: Option<&Project>,
    sources: &[ProjectObject],
    config: &GridArrayConfig,
) -> Vec<ProjectObject> {
    if sources.is_empty() {
        return vec![];
    }
    let Some((eff_h, eff_v)) = effective_grid_spacing(sources, config) else {
        return vec![];
    };

    let mut results = Vec::new();
    let mut copy_index: usize = 0;

    for row in 0..config.rows {
        for col in 0..config.cols {
            if row == 0 && col == 0 {
                continue; // Skip the original position
            }

            let (tx, ty) = grid_cell_translation(config, eff_h, eff_v, row, col);

            // Clone ALL source objects per position
            for (src_idx, source) in sources.iter().enumerate() {
                let mut obj = source.clone();
                obj.id = ObjectId::new();

                // Translation → bounds only (NO transform.tx/ty)
                let width = obj.bounds.width();
                let height = obj.bounds.height();
                obj.bounds.min.x += tx;
                obj.bounds.min.y += ty;
                obj.bounds.max.x = obj.bounds.min.x + width;
                obj.bounds.max.y = obj.bounds.min.y + height;

                // Mirror → transform only
                if config.mirror_alternate_cols && col % 2 == 1 {
                    obj.transform = Transform2D::scale(-1.0, 1.0).compose(&obj.transform);
                }
                if config.mirror_alternate_rows && row % 2 == 1 {
                    obj.transform = Transform2D::scale(1.0, -1.0).compose(&obj.transform);
                }

                // Random orientation → transform only
                if config.random_orientation {
                    let angle =
                        hash_rotation(config.random_seed, copy_index * sources.len() + src_idx);
                    obj.transform = Transform2D::rotate(angle).compose(&obj.transform);
                }

                // Auto-increment variable text
                if config.auto_increment_text {
                    apply_text_auto_increment(
                        project,
                        &mut obj,
                        copy_index + 1,
                        config.text_increment,
                    );
                }

                results.push(obj);
            }
            copy_index += 1;
        }
    }

    results
}

/// Configuration for circular array operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircularArrayConfig {
    pub count: u32,
    pub radius_mm: f64,
    pub rotate_copies: bool,
    /// Explicit center X (None = auto from combined bounds).
    #[serde(default)]
    pub center_x: Option<f64>,
    /// Explicit center Y (None = auto from combined bounds).
    #[serde(default)]
    pub center_y: Option<f64>,
    /// Start angle in degrees (default 0).
    #[serde(default)]
    pub start_angle_deg: f64,
    /// End angle in degrees (default 360).
    #[serde(default = "default_end_angle")]
    pub end_angle_deg: f64,
    /// Group all copies into one Group object (consumed by command layer).
    #[serde(default)]
    pub group_results: bool,
    /// Create VirtualClone copies instead of real copies (consumed by command layer).
    #[serde(default)]
    pub create_virtual: bool,
    /// Auto-increment variable text for Text objects with variable_text config.
    #[serde(default)]
    pub auto_increment_text: bool,
    /// Increment value for auto-increment text (default 1).
    #[serde(default = "default_text_increment")]
    pub text_increment: i64,
}

fn default_end_angle() -> f64 {
    360.0
}

/// Create a circular array of objects around a center point.
/// Takes multiple source objects and arrays them as a unit.
pub fn circular_array(
    sources: &[ProjectObject],
    config: &CircularArrayConfig,
) -> Vec<ProjectObject> {
    if config.count == 0 || sources.is_empty() {
        return vec![];
    }

    let cb = combined_bounds(sources);
    let center_x = config.center_x.unwrap_or((cb.min.x + cb.max.x) / 2.0);
    let center_y = config.center_y.unwrap_or((cb.min.y + cb.max.y) / 2.0);

    // Composite center of the selection — used to translate as a unit
    let group_cx = (cb.min.x + cb.max.x) / 2.0;
    let group_cy = (cb.min.y + cb.max.y) / 2.0;

    let total_angle = config.end_angle_deg - config.start_angle_deg;
    let is_full_circle = (total_angle.abs() - 360.0).abs() < 0.01;

    // Place every requested position evenly on the circle:
    // Full circle: step = total/count
    // Partial arc: step = total/(count-1), positions at endpoints included
    let angle_step_deg = if is_full_circle {
        total_angle / config.count as f64
    } else if config.count > 1 {
        total_angle / (config.count as f64 - 1.0)
    } else {
        0.0
    };

    let mut results = Vec::new();

    for i in 0..config.count {
        let angle_deg = config.start_angle_deg + i as f64 * angle_step_deg;
        let angle_rad = angle_deg * PI / 180.0;

        // Position on circle relative to center
        let offset_x = config.radius_mm * angle_rad.cos();
        let offset_y = config.radius_mm * angle_rad.sin();

        for source in sources {
            let mut obj = source.clone();
            // Position 0 keeps original IDs (caller uses these to update originals in-place)
            if i > 0 {
                obj.id = ObjectId::new();
            }

            // Translation in bounds only — offset from the group's composite center
            // so multi-object selections preserve internal spacing
            let dx = (center_x + offset_x) - group_cx;
            let dy = (center_y + offset_y) - group_cy;

            let width = obj.bounds.width();
            let height = obj.bounds.height();
            obj.bounds.min.x += dx;
            obj.bounds.min.y += dy;
            obj.bounds.max.x = obj.bounds.min.x + width;
            obj.bounds.max.y = obj.bounds.min.y + height;

            // Rotation in transform only — all positions including position 0
            // so that nonzero start_angle rotates the originals correctly
            if config.rotate_copies {
                obj.transform = Transform2D::rotate(angle_rad).compose(&obj.transform);
            }

            // Auto-increment variable text (position 0 keeps original text)
            if config.auto_increment_text && i > 0 {
                apply_text_auto_increment(None, &mut obj, i as usize, config.text_increment);
            }

            results.push(obj);
        }
    }

    results
}

/// Copy an object along a path at regular intervals.
pub fn copy_along_path(
    source: &ProjectObject,
    guide: &VecPath,
    spacing_mm: f64,
    rotate: bool,
    scale_copies: bool,
    final_scale_percent: f64,
) -> Vec<ProjectObject> {
    let polylines = flatten_vecpath(guide, DEFAULT_TOLERANCE_MM);

    if polylines.is_empty() {
        return vec![];
    }

    // Use first polyline for now
    let points = &polylines[0].points;
    if points.len() < 2 {
        return vec![];
    }

    // Compute arc length samples
    let samples = sample_path_at_intervals(points, spacing_mm);

    let mut results = Vec::new();

    let total_copies = samples.len();
    let final_scale = final_scale_percent / 100.0;

    for (index, (pos, tangent_angle)) in samples.into_iter().enumerate() {
        let mut obj = source.clone();
        obj.id = ObjectId::new();

        // Translation in bounds only (NO transform.tx/ty)
        let width = obj.bounds.width();
        let height = obj.bounds.height();
        let center_x = pos.x + width / 2.0;
        let center_y = pos.y + height / 2.0;
        let scale = if !scale_copies {
            1.0
        } else if total_copies <= 1 {
            final_scale
        } else {
            1.0 + (final_scale - 1.0) * (index as f64 / (total_copies as f64 - 1.0))
        };
        let scaled_width = width * scale;
        let scaled_height = height * scale;
        obj.bounds.min.x = center_x - scaled_width / 2.0;
        obj.bounds.min.y = center_y - scaled_height / 2.0;
        obj.bounds.max.x = obj.bounds.min.x + scaled_width;
        obj.bounds.max.y = obj.bounds.min.y + scaled_height;

        // Rotation in transform only
        if rotate {
            obj.transform = Transform2D::rotate(tangent_angle).compose(&obj.transform);
        }

        results.push(obj);
    }

    results
}

/// Apply auto-increment to a Text object's variable_text config.
/// `copy_index` is 1-based (first copy = 1, second = 2, etc.).
fn resolve_variable_text_for_copy(
    project: Option<&Project>,
    object: &ProjectObject,
    config: &VariableTextConfig,
    source: &variable_text::VariableTextSource,
    copy_index: usize,
) -> String {
    let config = VariableTextConfig {
        template: config.template.clone(),
        mode: config.mode,
        offset: config.offset,
        source: source.clone(),
    };

    if let Some(project) = project {
        return variable_text::resolve_text_in_project(project, object, &config, copy_index);
    }

    if matches!(config.mode, Some(VariableTextMode::Normal)) {
        return config.template;
    }

    let mut source = config.source;
    if source.end < source.start {
        std::mem::swap(&mut source.start, &mut source.end);
    }
    if matches!(
        config.mode,
        None | Some(VariableTextMode::SerialNumber) | Some(VariableTextMode::MergeCsv)
    ) {
        source.current = variable_text::advance_sequence_value(
            source.current,
            source.start,
            source.end,
            config.offset.unwrap_or(0),
        );
    }

    variable_text::resolve_text(&config.template, &source, copy_index)
}

fn apply_text_auto_increment(
    project: Option<&Project>,
    obj: &mut ProjectObject,
    copy_index: usize,
    text_increment: i64,
) {
    let object_snapshot = obj.clone();
    if let ObjectData::Text {
        ref mut content,
        ref mut variable_text,
        ..
    } = obj.data
    {
        if let Some(vt_config) = variable_text.as_ref() {
            let mut updated_source = vt_config.source.clone();

            // Bump serial start
            if let Some(start_str) = updated_source.field_defaults.get("_serial_start") {
                if let Ok(start) = start_str.parse::<i64>() {
                    let new_start = start + copy_index as i64 * text_increment;
                    updated_source
                        .field_defaults
                        .insert("_serial_start".to_string(), new_start.to_string());
                }
            }

            // Advance CSV row / sequence cursor
            let csv_row_count = if updated_source.csv_data.len() > 1 {
                updated_source.csv_data.len() - 1 // exclude header
            } else {
                0
            };
            if csv_row_count > 0 {
                updated_source.current =
                    ((updated_source.current as usize + copy_index) % csv_row_count) as i64;
            } else {
                updated_source.current += copy_index as i64 * text_increment;
            }

            // Pass copy_index as row — drives {Serial:params} progression.
            // CSV resolution reads from updated_source.current/currentRow (set above).
            let resolved = resolve_variable_text_for_copy(
                project,
                &object_snapshot,
                vt_config,
                &updated_source,
                copy_index,
            );
            *content = resolved;

            // Preserve updated config on copy
            *variable_text = Some(VariableTextConfig {
                template: vt_config.template.clone(),
                mode: vt_config.mode,
                offset: vt_config.offset,
                source: updated_source,
            });
        }
    }
}

fn sample_path_at_intervals(points: &[Point2D], interval: f64) -> Vec<(Point2D, f64)> {
    if points.len() < 2 || interval <= 0.0 {
        return vec![];
    }

    const EPSILON: f64 = 1e-9;
    let mut samples = Vec::new();
    let mut accumulated = 0.0;

    for i in 0..points.len() - 1 {
        let p1 = points[i];
        let p2 = points[i + 1];

        let dx = p2.x - p1.x;
        let dy = p2.y - p1.y;
        let segment_len = (dx * dx + dy * dy).sqrt();

        if segment_len < 1e-9 {
            continue;
        }

        let tangent = dy.atan2(dx);

        while accumulated + interval <= segment_len + EPSILON {
            accumulated += interval;
            let t = (accumulated / segment_len).min(1.0);
            let pos = Point2D {
                x: p1.x + dx * t,
                y: p1.y + dy * t,
            };
            samples.push((pos, tangent));
        }

        accumulated -= segment_len;
    }

    samples
}

/// Create a convex hull outline around objects (rubber-band effect).
pub fn rubber_band_outline(objects: &[ProjectObject]) -> VecPath {
    use geo::algorithm::convex_hull::ConvexHull;
    use geo::{Coord, MultiPoint, Point};

    if objects.is_empty() {
        return VecPath { subpaths: vec![] };
    }

    // Stretch the hull around the objects' ACTUAL world geometry, like a
    // rubber band: a triangle frames as a triangle, not its bounding box.
    // Rasters/text and anything without vector geometry contribute their
    // bounds corners instead.
    let mut points = Vec::new();
    for obj in objects {
        let mut pushed_geometry = false;
        if let Some(path) = crate::vector::convert::object_to_world_vecpath(obj) {
            for polyline in crate::vector::flatten::flatten_vecpath(&path, 0.2) {
                for p in &polyline.points {
                    points.push(Point::new(p.x, p.y));
                    pushed_geometry = true;
                }
            }
        }
        if !pushed_geometry {
            points.push(Point::new(obj.bounds.min.x, obj.bounds.min.y));
            points.push(Point::new(obj.bounds.max.x, obj.bounds.min.y));
            points.push(Point::new(obj.bounds.max.x, obj.bounds.max.y));
            points.push(Point::new(obj.bounds.min.x, obj.bounds.max.y));
        }
    }

    let multi_point = MultiPoint::new(points);
    let hull = multi_point.convex_hull();

    // Convert polygon to VecPath
    use beambench_common::path::{PathCommand, SubPath};

    let coords: Vec<&Coord<f64>> = hull.exterior().coords().collect();

    if coords.len() < 3 {
        return VecPath { subpaths: vec![] };
    }

    let mut commands = Vec::new();
    commands.push(PathCommand::MoveTo {
        x: coords[0].x,
        y: coords[0].y,
    });

    for coord in &coords[1..] {
        commands.push(PathCommand::LineTo {
            x: coord.x,
            y: coord.y,
        });
    }

    commands.push(PathCommand::Close);

    VecPath {
        subpaths: vec![SubPath {
            commands,
            closed: true,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::LayerId;
    use crate::object::{ObjectData, ShapeKind};
    use beambench_common::Bounds;

    fn make_test_object(x: f64, y: f64, w: f64, h: f64) -> ProjectObject {
        ProjectObject {
            id: ObjectId::new(),
            name: "test".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x, y },
                max: Point2D { x: x + w, y: y + h },
            },
            transform: Transform2D::identity(),
            layer_id: LayerId::new(),
            data: ObjectData::Shape {
                kind: ShapeKind::Rectangle,
                width: w,
                height: h,
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

    // ── Grid Array Tests ─────────────────────────────

    #[test]
    fn grid_array_creates_copies() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 3,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 15.0,
            v_spacing_mm: 15.0,
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

        let result = grid_array(&[source], &config);

        // 2x3 grid minus original = 5 copies
        assert_eq!(result.len(), 5);
    }

    #[test]
    fn grid_array_no_double_translation() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 0.0,
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

        let result = grid_array(&[source], &config);
        assert_eq!(result.len(), 1);

        let copy = &result[0];
        // Transform should have NO translation — translation is in bounds only
        assert!(
            copy.transform.tx.abs() < f64::EPSILON,
            "transform.tx should be 0, got {}",
            copy.transform.tx
        );
        assert!(
            copy.transform.ty.abs() < f64::EPSILON,
            "transform.ty should be 0, got {}",
            copy.transform.ty
        );
        // Bounds should reflect the 20mm offset
        assert!(
            (copy.bounds.min.x - 20.0).abs() < f64::EPSILON,
            "bounds.min.x should be 20, got {}",
            copy.bounds.min.x
        );
        assert!(
            (copy.bounds.max.x - 30.0).abs() < f64::EPSILON,
            "bounds.max.x should be 30, got {}",
            copy.bounds.max.x
        );
    }

    #[test]
    fn grid_array_edge_to_edge_spacing() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 5.0,
            v_spacing_mm: 0.0,
            spacing_mode: SpacingMode::EdgeToEdge,
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

        let result = grid_array(&[source], &config);
        assert_eq!(result.len(), 1);

        // EdgeToEdge: eff_h = 5 + 10 = 15, so col=1 at x=15
        let copy = &result[0];
        assert!(
            (copy.bounds.min.x - 15.0).abs() < f64::EPSILON,
            "EdgeToEdge: copy should be at x=15, got {}",
            copy.bounds.min.x
        );
    }

    #[test]
    fn grid_array_mirror_alternate_cols() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 3,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 15.0,
            v_spacing_mm: 0.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: true,
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

        let result = grid_array(&[source], &config);
        assert_eq!(result.len(), 2);

        // Col 1 (odd): should have scale(-1,1) in transform
        assert!(
            (result[0].transform.a - (-1.0)).abs() < f64::EPSILON,
            "Odd col transform.a should be -1, got {}",
            result[0].transform.a
        );

        // Col 2 (even): no mirror
        assert!(
            (result[1].transform.a - 1.0).abs() < f64::EPSILON,
            "Even col transform.a should be 1, got {}",
            result[1].transform.a
        );
    }

    #[test]
    fn grid_array_x_col_shift() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 20.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 5.0,
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

        let result = grid_array(&[source], &config);
        // row=1, col=0: tx = 0 * 20 + 5 * 1 = 5
        let row1_col0 = result.iter().find(|o| {
            (o.bounds.min.y - 20.0).abs() < f64::EPSILON
                && (o.bounds.min.x - 5.0).abs() < f64::EPSILON
        });
        assert!(
            row1_col0.is_some(),
            "row=1 col=0 should be shifted by x_col_shift: expected x=5"
        );
    }

    #[test]
    fn grid_array_half_shift() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 20.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: true,
            reverse_h: false,
            reverse_v: false,
            random_orientation: false,
            random_seed: 0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = grid_array(&[source], &config);
        // row=1, col=0: ty=20, tx=eff_h/2 = 10 (brickwork offset)
        let row1_col0 = result
            .iter()
            .find(|o| (o.bounds.min.y - 20.0).abs() < f64::EPSILON);
        assert!(row1_col0.is_some());
        let shifted = row1_col0.unwrap();
        assert!(
            (shifted.bounds.min.x - 10.0).abs() < f64::EPSILON,
            "Half-shift: odd row should be offset by eff_h/2=10, got x={}",
            shifted.bounds.min.x
        );
    }

    #[test]
    fn grid_array_reverse_h() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 0.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: false,
            reverse_h: true,
            reverse_v: false,
            random_orientation: false,
            random_seed: 0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = grid_array(&[source], &config);
        assert_eq!(result.len(), 1);
        // reverse_h: col=1 → effective_col = -1, tx = -20
        assert!(
            (result[0].bounds.min.x - (-20.0)).abs() < f64::EPSILON,
            "reverse_h: copy should be at x=-20, got {}",
            result[0].bounds.min.x
        );
    }

    #[test]
    fn grid_array_random_orientation_deterministic() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 20.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: false,
            reverse_h: false,
            reverse_v: false,
            random_orientation: true,
            random_seed: 42,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let r1 = grid_array(&[source.clone()], &config);
        let r2 = grid_array(&[source], &config);

        // Same seed should produce same transforms
        assert_eq!(r1.len(), r2.len());
        for (a, b) in r1.iter().zip(r2.iter()) {
            assert!(
                (a.transform.a - b.transform.a).abs() < f64::EPSILON,
                "Same seed should produce same rotation"
            );
        }
    }

    #[test]
    fn grid_array_multi_object() {
        let src_a = make_test_object(0.0, 0.0, 10.0, 10.0);
        let src_b = make_test_object(15.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 40.0,
            v_spacing_mm: 0.0,
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

        let result = grid_array(&[src_a, src_b], &config);
        // 1 position (col=1) × 2 sources = 2 copies
        assert_eq!(result.len(), 2);
        // Both copies should be shifted by 40mm
        assert!(
            (result[0].bounds.min.x - 40.0).abs() < f64::EPSILON,
            "First source copy at x=40, got {}",
            result[0].bounds.min.x
        );
        assert!(
            (result[1].bounds.min.x - 55.0).abs() < f64::EPSILON,
            "Second source copy at x=55, got {}",
            result[1].bounds.min.x
        );
    }

    #[test]
    fn grid_array_single_cell() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 1,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 15.0,
            v_spacing_mm: 15.0,
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

        let result = grid_array(&[source], &config);
        // 1x1 grid minus original = 0 copies
        assert_eq!(result.len(), 0);
    }

    // ── Circular Array Tests ─────────────────────────────

    #[test]
    fn circular_array_creates_copies() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 6,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        // All count positions on the circle (position 0 = original moved)
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn circular_array_no_double_translation() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 4,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        for copy in &result {
            // Transform should NOT have translation — only rotation
            assert!(
                copy.transform.tx.abs() < f64::EPSILON,
                "circular copy transform.tx should be 0, got {}",
                copy.transform.tx
            );
            assert!(
                copy.transform.ty.abs() < f64::EPSILON,
                "circular copy transform.ty should be 0, got {}",
                copy.transform.ty
            );
        }
    }

    #[test]
    fn circular_array_explicit_center() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 4,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: Some(100.0),
            center_y: Some(100.0),
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        assert_eq!(result.len(), 4);

        // Position 0 at 0°: offset_x=50, offset_y=0 → center (100+50, 100) = (150, 100)
        let pos0 = &result[0];
        let pos0_cx = (pos0.bounds.min.x + pos0.bounds.max.x) / 2.0;
        assert!(
            (pos0_cx - 150.0).abs() < 1.0,
            "Explicit center: pos-0 x-center should be ~150, got {}",
            pos0_cx
        );

        // Position 1 at 90°: offset_x=0, offset_y=50 → center (100, 150)
        let pos1 = &result[1];
        let pos1_cy = (pos1.bounds.min.y + pos1.bounds.max.y) / 2.0;
        assert!(
            (pos1_cy - 150.0).abs() < 1.0,
            "Explicit center: pos-1 y-center should be ~150, got {}",
            pos1_cy
        );
    }

    #[test]
    fn circular_array_partial_arc() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 4,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 180.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        // Partial arc: all 4 positions on the arc
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn circular_array_multi_object() {
        let src_a = make_test_object(0.0, 0.0, 10.0, 10.0);
        let src_b = make_test_object(15.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 3,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[src_a, src_b], &config);
        // 3 positions × 2 sources = 6 items (position 0 = originals moved)
        assert_eq!(result.len(), 6);

        // Verify internal spacing is preserved at every position.
        // Original distance between A and B centers: 15mm (A center 5, B center 20).
        let original_dist = 15.0_f64;
        for pos in 0..3 {
            let a = &result[pos * 2];
            let b = &result[pos * 2 + 1];
            let a_cx = (a.bounds.min.x + a.bounds.max.x) / 2.0;
            let a_cy = (a.bounds.min.y + a.bounds.max.y) / 2.0;
            let b_cx = (b.bounds.min.x + b.bounds.max.x) / 2.0;
            let b_cy = (b.bounds.min.y + b.bounds.max.y) / 2.0;
            let dist = ((b_cx - a_cx).powi(2) + (b_cy - a_cy).powi(2)).sqrt();
            assert!(
                (dist - original_dist).abs() < 0.01,
                "Position {pos}: internal spacing should be {original_dist}mm, got {dist}mm"
            );
        }
    }

    #[test]
    fn circular_array_with_rotation() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 8,
            radius_mm: 30.0,
            rotate_copies: true,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        assert_eq!(result.len(), 8);

        // Extract rotation angle from transform matrix: atan2(b, a)
        let rot0 = result[0].transform.b.atan2(result[0].transform.a);
        // Position 0 at 0° should have rotation = 0
        assert!(
            rot0.abs() < 0.01,
            "Position 0 at 0° should have ~0 rotation, got {rot0}"
        );
        // Position 1 at 45° should have rotation = π/4
        let rot1 = result[1].transform.b.atan2(result[1].transform.a);
        let expected_45 = std::f64::consts::PI / 4.0;
        assert!(
            (rot1 - expected_45).abs() < 0.01,
            "Position 1 at 45° should have ~π/4 rotation, got {rot1}"
        );
    }

    #[test]
    fn circular_array_nonzero_start_angle_rotates_position_zero() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 4,
            radius_mm: 50.0,
            rotate_copies: true,
            center_x: None,
            center_y: None,
            start_angle_deg: 90.0,
            end_angle_deg: 450.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        assert_eq!(result.len(), 4);

        // Position 0 starts at 90° — should have rotation = π/2
        let rot0 = result[0].transform.b.atan2(result[0].transform.a);
        let expected_90 = std::f64::consts::PI / 2.0;
        assert!(
            (rot0 - expected_90).abs() < 0.01,
            "Position 0 at start_angle=90° should have ~π/2 rotation, got {rot0}"
        );
        // Position 1 at 180° — should have rotation = π
        let rot1 = result[1].transform.b.atan2(result[1].transform.a);
        let expected_180 = std::f64::consts::PI;
        assert!(
            (rot1 - expected_180).abs() < 0.01,
            "Position 1 at 180° should have ~π rotation, got {rot1}"
        );
    }

    #[test]
    fn circular_array_zero_count() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = CircularArrayConfig {
            count: 0,
            radius_mm: 50.0,
            rotate_copies: false,
            center_x: None,
            center_y: None,
            start_angle_deg: 0.0,
            end_angle_deg: 360.0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let result = circular_array(&[source], &config);
        assert!(result.is_empty());
    }

    // ── Copy Along Path Tests ─────────────────────────────

    #[test]
    fn copy_along_path_creates_copies() {
        let source = make_test_object(0.0, 0.0, 5.0, 5.0);
        let guide = VecPath::parse_svg_d("M0 0 L100 0");

        let result = copy_along_path(&source, &guide, 10.0, false, false, 100.0);

        // Should create multiple copies along 100mm line
        assert!(!result.is_empty());
        assert!(result.len() >= 5);
    }

    #[test]
    fn copy_along_path_no_double_translation() {
        let source = make_test_object(0.0, 0.0, 5.0, 5.0);
        let guide = VecPath::parse_svg_d("M0 0 L100 0");

        let result = copy_along_path(&source, &guide, 20.0, false, false, 100.0);
        assert!(!result.is_empty());

        for copy in &result {
            // Transform should NOT have translation
            assert!(
                copy.transform.tx.abs() < f64::EPSILON,
                "copy_along_path transform.tx should be 0, got {}",
                copy.transform.tx
            );
            assert!(
                copy.transform.ty.abs() < f64::EPSILON,
                "copy_along_path transform.ty should be 0, got {}",
                copy.transform.ty
            );
        }
    }

    #[test]
    fn copy_along_path_with_rotation() {
        let source = make_test_object(0.0, 0.0, 5.0, 5.0);
        let guide = VecPath::parse_svg_d("M0 0 L50 50");

        let result = copy_along_path(&source, &guide, 10.0, true, false, 100.0);

        assert!(!result.is_empty());
    }

    #[test]
    fn copy_along_path_scales_last_copy_to_requested_percent() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let guide = VecPath::parse_svg_d("M0 0 L30 0");

        let result = copy_along_path(&source, &guide, 10.0, false, true, 50.0);

        assert_eq!(result.len(), 3);
        assert!((result[0].bounds.width() - 10.0).abs() < 0.01);
        assert!((result[1].bounds.width() - 7.5).abs() < 0.01);
        assert!((result[2].bounds.width() - 5.0).abs() < 0.01);
    }

    #[test]
    fn copy_along_path_scales_to_two_hundred_percent_over_four_copies() {
        let source = make_test_object(0.0, 0.0, 12.0, 12.0);
        let guide = VecPath::parse_svg_d("M0 0 L40 0");

        let result = copy_along_path(&source, &guide, 10.0, false, true, 200.0);

        assert_eq!(result.len(), 4);
        assert!((result[0].bounds.width() - 12.0).abs() < 0.02);
        assert!((result[1].bounds.width() - 16.0).abs() < 0.02);
        assert!((result[2].bounds.width() - 20.0).abs() < 0.02);
        assert!((result[3].bounds.width() - 24.0).abs() < 0.02);
    }

    #[test]
    fn copy_along_path_single_copy_uses_final_scale_directly() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let guide = VecPath::parse_svg_d("M0 0 L10 0");

        let result = copy_along_path(&source, &guide, 10.0, false, true, 50.0);

        assert_eq!(result.len(), 1);
        assert!((result[0].bounds.width() - 5.0).abs() < 0.01);
        assert!((result[0].bounds.height() - 5.0).abs() < 0.01);
    }

    #[test]
    fn copy_along_path_scaling_preserves_copy_center() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let guide = VecPath::parse_svg_d("M0 0 L30 0");

        let unscaled = copy_along_path(&source, &guide, 10.0, false, false, 100.0);
        let scaled = copy_along_path(&source, &guide, 10.0, false, true, 50.0);

        assert_eq!(unscaled.len(), scaled.len());
        for (base, scaled_copy) in unscaled.iter().zip(scaled.iter()) {
            let base_cx = (base.bounds.min.x + base.bounds.max.x) / 2.0;
            let base_cy = (base.bounds.min.y + base.bounds.max.y) / 2.0;
            let scaled_cx = (scaled_copy.bounds.min.x + scaled_copy.bounds.max.x) / 2.0;
            let scaled_cy = (scaled_copy.bounds.min.y + scaled_copy.bounds.max.y) / 2.0;
            assert!((base_cx - scaled_cx).abs() < 0.01);
            assert!((base_cy - scaled_cy).abs() < 0.01);
        }
    }

    #[test]
    fn copy_along_path_scale_disabled_matches_current_output() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let guide = VecPath::parse_svg_d("M0 0 L30 0");

        let disabled = copy_along_path(&source, &guide, 10.0, false, false, 50.0);
        let one_hundred = copy_along_path(&source, &guide, 10.0, false, true, 100.0);

        assert_eq!(disabled.len(), one_hundred.len());
        for (a, b) in disabled.iter().zip(one_hundred.iter()) {
            assert!((a.bounds.min.x - b.bounds.min.x).abs() < 0.01);
            assert!((a.bounds.max.x - b.bounds.max.x).abs() < 0.01);
            assert!((a.bounds.min.y - b.bounds.min.y).abs() < 0.01);
            assert!((a.bounds.max.y - b.bounds.max.y).abs() < 0.01);
        }
    }

    #[test]
    fn fit_grid_array_counts_uses_total_axes_and_exact_boundary() {
        let source = make_test_object(0.0, 0.0, 5.0, 5.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 2,
            sizing_mode_x: GridArraySizingMode::Total,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: Some(15.0),
            total_height_mm: None,
            h_spacing_mm: 5.0,
            v_spacing_mm: 5.0,
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

        let fitted = fit_grid_array_counts(&[source], &config, 100);

        assert_eq!(fitted.0, 2);
        assert_eq!(fitted.1, 3);
    }

    #[test]
    fn fit_grid_array_counts_clamps_to_one_when_total_width_is_smaller_than_source() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 5,
            sizing_mode_x: GridArraySizingMode::Total,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: Some(4.0),
            total_height_mm: None,
            h_spacing_mm: 5.0,
            v_spacing_mm: 5.0,
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

        let fitted = fit_grid_array_counts(&[source], &config, 100);

        assert_eq!(fitted.0, 2);
        assert_eq!(fitted.1, 1);
    }

    #[test]
    fn fit_grid_array_counts_accounts_for_half_shift_when_fitting_width() {
        let source = make_test_object(0.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 2,
            cols: 5,
            sizing_mode_x: GridArraySizingMode::Total,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: Some(20.0),
            total_height_mm: None,
            h_spacing_mm: 10.0,
            v_spacing_mm: 10.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: true,
            reverse_h: false,
            reverse_v: false,
            random_orientation: false,
            random_seed: 0,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let fitted = fit_grid_array_counts(&[source], &config, 100);

        assert_eq!(fitted.1, 1);
    }

    #[test]
    fn fit_grid_array_counts_handles_multi_object_sources() {
        let src_a = make_test_object(0.0, 0.0, 10.0, 10.0);
        let src_b = make_test_object(15.0, 0.0, 10.0, 10.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 5,
            sizing_mode_x: GridArraySizingMode::Total,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: Some(65.0),
            total_height_mm: None,
            h_spacing_mm: 20.0,
            v_spacing_mm: 0.0,
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

        let fitted = fit_grid_array_counts(&[src_a, src_b], &config, 100);

        assert_eq!(fitted.1, 3);
    }

    #[test]
    fn fit_grid_array_counts_runs_with_random_orientation_enabled() {
        let source = make_test_object(0.0, 0.0, 10.0, 20.0);
        let config = GridArrayConfig {
            rows: 1,
            cols: 5,
            sizing_mode_x: GridArraySizingMode::Total,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: Some(40.0),
            total_height_mm: None,
            h_spacing_mm: 10.0,
            v_spacing_mm: 0.0,
            spacing_mode: SpacingMode::CenterToCenter,
            mirror_alternate_cols: false,
            mirror_alternate_rows: false,
            x_col_shift_mm: 0.0,
            y_row_shift_mm: 0.0,
            half_shift: false,
            reverse_h: false,
            reverse_v: false,
            random_orientation: true,
            random_seed: 42,
            group_results: false,
            create_virtual: false,
            auto_increment_text: false,
            text_increment: 1,
        };

        let fitted = fit_grid_array_counts(&[source], &config, 100);

        assert_eq!(fitted.1, 4);
    }

    // ── Rubber Band Outline Tests ─────────────────────────────

    #[test]
    fn rubber_band_outline_creates_hull() {
        let obj1 = make_test_object(0.0, 0.0, 10.0, 10.0);
        let obj2 = make_test_object(20.0, 20.0, 10.0, 10.0);

        let result = rubber_band_outline(&[obj1, obj2]);

        assert!(!result.is_empty());
        assert!(result.subpaths[0].closed);
    }

    #[test]
    fn rubber_band_outline_empty_objects() {
        let result = rubber_band_outline(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn rubber_band_outline_follows_shape_geometry_not_bounds() {
        // A triangle (ellipse would also work): the hull must hug the actual
        // geometry. With the old bounds-corner hull, the outline was just the
        // bounding rectangle and "Outline" framing looked identical to
        // rectangular framing.
        let mut tri = make_test_object(0.0, 0.0, 100.0, 100.0);
        tri.data = crate::ObjectData::VectorPath {
            path_data: "M 50 0 L 100 100 L 0 100 Z".to_string(),
            closed: true,
            ruler_guide_axis: None,
        };

        let result = rubber_band_outline(&[tri]);
        assert!(!result.is_empty());

        // The bounds corner (0,0) (top-left of the AABB) is NOT part of the
        // triangle's geometry, so it must not appear as a hull vertex.
        let has_bounds_corner = result.subpaths[0].commands.iter().any(|cmd| match cmd {
            beambench_common::path::PathCommand::MoveTo { x, y }
            | beambench_common::path::PathCommand::LineTo { x, y } => {
                x.abs() < 1.0 && y.abs() < 1.0
            }
            _ => false,
        });
        assert!(
            !has_bounds_corner,
            "hull must follow the triangle, not its bounding box: {result:?}"
        );
    }

    // ── Hash Rotation Tests ─────────────────────────────

    #[test]
    fn hash_rotation_produces_quadrant_angles() {
        for idx in 0..20 {
            let angle = hash_rotation(42, idx);
            let valid = [0.0, PI / 2.0, PI, 3.0 * PI / 2.0];
            assert!(
                valid.iter().any(|&v| (angle - v).abs() < f64::EPSILON),
                "hash_rotation({idx}) = {angle} is not a valid quadrant angle"
            );
        }
    }

    // ── Auto-Increment Text Tests ─────────────────────────────

    #[test]
    fn auto_increment_text_serial_in_grid() {
        use crate::variable_text::{VariableTextConfig, VariableTextSource};
        use std::collections::HashMap;

        let mut defaults = HashMap::new();
        defaults.insert("_serial_start".to_string(), "1".to_string());
        defaults.insert("_serial_increment".to_string(), "1".to_string());
        defaults.insert("_serial_padding".to_string(), "3".to_string());

        let source = ProjectObject {
            id: ObjectId::new(),
            name: "serial_text".into(),
            visible: true,
            locked: false,
            bounds: Bounds {
                min: Point2D { x: 0.0, y: 0.0 },
                max: Point2D { x: 20.0, y: 10.0 },
            },
            transform: Transform2D::identity(),
            layer_id: LayerId::new(),
            data: ObjectData::Text {
                content: "SN-001".into(),
                font_family: "Arial".into(),
                font_size_mm: 5.0,
                alignment: crate::object::TextAlignment::Left,
                alignment_v: crate::object::TextAlignmentV::Top,
                bold: false,
                italic: false,
                upper_case: false,
                welded: false,
                h_spacing: 0.0,
                v_spacing: 0.0,
                on_path: false,
                path_offset: 0.0,
                distort: false,
                layout_mode: crate::object::TextLayoutMode::Straight,
                rtl: false,
                bend_radius: 0.0,
                transform_style: crate::object::TextTransformStyle::None,
                transform_curve: 0.0,
                circle_placement: crate::object::TextCirclePlacement::TopOutside,
                resolved_font_source: None,
                resolved_font_key: None,
                resolved_path_data: None,
                missing_font: false,
                missing_glyphs: Vec::new(),
                guide_path_id: None,
                variable_text: Some(VariableTextConfig {
                    template: "SN-{Serial}".into(),
                    mode: None,
                    offset: None,
                    source: VariableTextSource {
                        csv_path: None,
                        csv_data: vec![],
                        field_defaults: defaults,
                        current: 1,
                        start: 1,
                        end: 1,
                        advance_by: 1,
                        auto_advance: false,
                        total_copies: 1,
                    },
                }),
                max_width: None,
                squeeze: false,
                ignore_empty_vars: false,
            },
            z_index: 0,
            lock_aspect_ratio: false,
            power_scale: 1.0,
            priority: 0,
            created_at: chrono::Utc::now(),
            tabs: Vec::new(),
            start_point_edits: Vec::new(),
        };

        let config = GridArrayConfig {
            rows: 1,
            cols: 4,
            sizing_mode_x: GridArraySizingMode::Count,
            sizing_mode_y: GridArraySizingMode::Count,
            total_width_mm: None,
            total_height_mm: None,
            h_spacing_mm: 25.0,
            v_spacing_mm: 0.0,
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
            auto_increment_text: true,
            text_increment: 1,
        };

        let result = grid_array(&[source], &config);
        assert_eq!(result.len(), 3);

        // Extract content from each copy
        let contents: Vec<String> = result
            .iter()
            .filter_map(|o| {
                if let ObjectData::Text { ref content, .. } = o.data {
                    Some(content.clone())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(contents[0], "SN-002"); // copy_index=1 → start+1=2
        assert_eq!(contents[1], "SN-003"); // copy_index=2 → start+2=3
        assert_eq!(contents[2], "SN-004"); // copy_index=3 → start+3=4
    }
}
