//! Potrace-based bitmap-to-vector trace with bitmap preparation and
//! post-trace optimization tuned for lower node counts.

use std::time::Instant;

use beambench_common::geometry::Point2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};
use image::{GrayImage, Luma, imageops::FilterType};

use crate::potrace::{
    bitmap::bm_to_pathlist,
    convert::paths_to_compound_vecpaths,
    curve::{opticurve, reverse, smooth},
    polygon::{adjust_vertices, bestpolygon, calc_lon, calc_sums},
    types::{InternalPath, TurnPolicy},
};
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};

const EPSILON: f64 = 1e-6;
const DEFAULT_OPTIMIZER_RUN_LIMIT: usize = 8;
const CUBIC_HANDLE_SCALES: [f64; 5] = [0.55, 0.8, 1.0, 1.2, 1.45];

/// Configuration for potrace-based image tracing.
pub struct TraceConfig {
    /// Upper bound of brightness range to trace (0-255). Default: 128.
    pub threshold: u8,
    /// Lower bound of brightness range to trace (0-255). Default: 0.
    /// Pixels with brightness in [cutoff, threshold] are foreground.
    pub cutoff: u8,
    /// Minimum contour area to keep (filters noise). Default: 2.
    pub turdsize: u32,
    /// Turn policy for resolving ambiguous boundaries.
    pub turnpolicy: TurnPolicy,
    /// Corner detection threshold. 0.0 = all corners, 1.334 = all curves. Default: 1.0.
    pub alphamax: f64,
    /// Enable curve optimization (segment merging). Default: true.
    pub opticurve: bool,
    /// Curve optimization tolerance. Higher = more aggressive merging. Default: 0.2.
    pub opttolerance: f64,
    /// If true, invert the image before tracing. Used when the service
    /// layer extracts the alpha channel (transparent=background).
    pub invert: bool,
    /// Sketch trace mode — strengthen local normalization before thresholding
    /// so nearby pixels influence the decision more heavily.
    pub sketch_trace: bool,
    /// Radius of the light denoise blur applied before thresholding.
    pub preblur_radius: f64,
    /// Radius for local background normalization.
    pub local_normalization_radius: u32,
    /// Blend amount of local normalization. Higher suppresses uneven lighting more aggressively.
    pub local_normalization_strength: f64,
    /// Optional supersample factor for antialias cleanup before thresholding.
    pub supersample_factor: u32,
    /// Area threshold for binary despeckle before Potrace.
    pub despeckle_area_threshold: u32,
    /// Maximum allowed boundary deviation when merging spans in the post-fit optimizer.
    pub post_fit_max_deviation: f64,
    /// RMS error target for the post-fit optimizer.
    pub post_fit_rms_target: f64,
    /// Preserve corners when tangent changes exceed this angle.
    pub corner_preservation_angle_deg: f64,
    /// Maximum optimizer search span used by the post-fit simplifier.
    pub optimizer_run_limit: usize,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            threshold: 128,
            cutoff: 0,
            turdsize: 2,
            turnpolicy: TurnPolicy::Minority,
            alphamax: 1.0,
            opticurve: true,
            opttolerance: 0.2,
            invert: false,
            sketch_trace: false,
            preblur_radius: 0.85,
            local_normalization_radius: 4,
            local_normalization_strength: 0.18,
            supersample_factor: 2,
            despeckle_area_threshold: 2,
            post_fit_max_deviation: 0.85,
            post_fit_rms_target: 0.30,
            corner_preservation_angle_deg: 34.0,
            optimizer_run_limit: DEFAULT_OPTIMIZER_RUN_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceProcessingBucket {
    Interactive,
    Moderate,
    Heavy,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceBenchmark {
    pub path_count: usize,
    pub anchor_count_before: usize,
    pub anchor_count_after: usize,
    pub max_deviation: f64,
    pub rms_deviation: f64,
    pub processing_bucket: TraceProcessingBucket,
}

#[derive(Debug, Clone)]
pub struct TraceResult {
    pub paths: Vec<VecPath>,
    pub benchmark: TraceBenchmark,
}

#[derive(Debug, Clone)]
struct PreparedTraceBitmap {
    width: u32,
    height: u32,
    pixels: Vec<Vec<bool>>,
}

#[derive(Debug, Clone)]
struct Segment {
    start: Point2D,
    command: PathCommand,
}

impl Segment {
    fn end(&self) -> Point2D {
        match self.command {
            PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => Point2D::new(x, y),
            PathCommand::MoveTo { x, y } => Point2D::new(x, y),
            PathCommand::Close => self.start,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct ErrorStats {
    max: f64,
    rms: f64,
}

/// Trace a grayscale bitmap image to vector outlines using the full Beam Bench
/// trace pipeline.
pub fn trace_image(image: &GrayImage, config: &TraceConfig) -> Vec<VecPath> {
    trace_image_with_report(image, config).paths
}

/// Trace a grayscale bitmap image for interactive preview. This skips
/// regression-only quality accounting so preview refreshes do not pay for
/// expensive error analysis on every slider change.
pub fn trace_image_preview_fast(image: &GrayImage, config: &TraceConfig) -> Vec<VecPath> {
    let prepared = prepare_bitmap(image, config);
    let potrace_paths = trace_bitmap_with_potrace(&prepared, config);
    let traced_paths = paths_to_compound_vecpaths(&potrace_paths);
    optimize_traced_vecpaths(traced_paths, config)
}

/// Trace a grayscale bitmap image for interactive preview with cooperative
/// cancellation checkpoints between major pipeline stages.
pub fn trace_image_preview_fast_cancellable<F>(
    image: &GrayImage,
    config: &TraceConfig,
    should_cancel: F,
) -> Option<Vec<VecPath>>
where
    F: Fn() -> bool,
{
    if should_cancel() {
        return None;
    }
    let prepared = prepare_bitmap(image, config);
    if should_cancel() {
        return None;
    }
    let potrace_paths = trace_bitmap_with_potrace(&prepared, config);
    if should_cancel() {
        return None;
    }
    let traced_paths = paths_to_compound_vecpaths(&potrace_paths);
    if should_cancel() {
        return None;
    }
    optimize_traced_vecpaths_with_cancel(traced_paths, config, &should_cancel)
}

/// Trace a grayscale bitmap image and return internal quality metrics useful
/// for regression tests.
pub fn trace_image_with_report(image: &GrayImage, config: &TraceConfig) -> TraceResult {
    let started_at = Instant::now();
    let prepared = prepare_bitmap(image, config);
    let potrace_paths = trace_bitmap_with_potrace(&prepared, config);
    let traced_paths = paths_to_compound_vecpaths(&potrace_paths);
    let anchor_count_before = anchor_count(&traced_paths);
    let optimized_paths = optimize_traced_vecpaths(traced_paths.clone(), config);
    let error_stats = compute_path_set_error(&traced_paths, &optimized_paths);
    let benchmark = TraceBenchmark {
        path_count: optimized_paths.len(),
        anchor_count_before,
        anchor_count_after: anchor_count(&optimized_paths),
        max_deviation: error_stats.max,
        rms_deviation: error_stats.rms,
        processing_bucket: processing_bucket_for(started_at.elapsed().as_millis()),
    };

    TraceResult {
        paths: optimized_paths,
        benchmark,
    }
}

fn processing_bucket_for(elapsed_ms: u128) -> TraceProcessingBucket {
    if elapsed_ms <= 80 {
        TraceProcessingBucket::Interactive
    } else if elapsed_ms <= 250 {
        TraceProcessingBucket::Moderate
    } else {
        TraceProcessingBucket::Heavy
    }
}

fn prepare_bitmap(image: &GrayImage, config: &TraceConfig) -> PreparedTraceBitmap {
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return PreparedTraceBitmap {
            width,
            height,
            pixels: vec![],
        };
    }

    let mut prepared = grayscale_with_optional_invert(image, config.invert);
    let supersample_factor = effective_supersample_factor(width, height, config.supersample_factor);
    if supersample_factor > 1 {
        prepared = image::imageops::resize(
            &prepared,
            width * supersample_factor,
            height * supersample_factor,
            FilterType::Triangle,
        );
    }

    let normalization_strength = if config.sketch_trace {
        config.local_normalization_strength.max(0.72)
    } else {
        config.local_normalization_strength
    };
    let normalization_radius = if config.sketch_trace {
        config.local_normalization_radius.max(6)
    } else {
        config.local_normalization_radius
    };
    if normalization_strength > 0.0 && normalization_radius > 0 {
        prepared = local_normalize(
            &prepared,
            normalization_radius * supersample_factor.max(1),
            normalization_strength,
        );
    }

    let blur_radius = if config.sketch_trace {
        config.preblur_radius.max(1.15)
    } else {
        config.preblur_radius
    };
    if blur_radius > 0.5 {
        prepared = box_blur(&prepared, blur_radius.round().max(1.0) as u32);
    }

    if supersample_factor > 1 {
        prepared = downsample_mean(&prepared, supersample_factor, width, height);
    }

    let mut pixels = vec![vec![false; width as usize]; height as usize];
    for y in 0..height {
        for x in 0..width {
            let val = prepared.get_pixel(x, y)[0];
            pixels[y as usize][x as usize] = val >= config.cutoff && val <= config.threshold;
        }
    }

    despeckle_bitmap(
        &mut pixels,
        config
            .despeckle_area_threshold
            .max(config.turdsize)
            .max(if config.sketch_trace { 3 } else { 1 }),
    );

    PreparedTraceBitmap {
        width,
        height,
        pixels,
    }
}

fn trace_bitmap_with_potrace(
    prepared: &PreparedTraceBitmap,
    config: &TraceConfig,
) -> Vec<InternalPath> {
    if prepared.width == 0 || prepared.height == 0 {
        return vec![];
    }

    let mut bm = vec![vec![false; (prepared.width + 1) as usize]; (prepared.height + 1) as usize];
    for y in 0..prepared.height {
        for x in 0..prepared.width {
            bm[y as usize][x as usize] = prepared.pixels[y as usize][x as usize];
        }
    }

    let mut paths = bm_to_pathlist(&mut bm, config.turdsize as i64, config.turnpolicy);
    if paths.is_empty() {
        return vec![];
    }

    for path in &mut paths {
        calc_sums(path);
        calc_lon(path);
        bestpolygon(path);
        adjust_vertices(path);

        if let Some(ref mut curve) = path.curve {
            if !path.sign {
                reverse(curve);
            }
            smooth(curve, config.alphamax);

            if config.opticurve {
                opticurve(path, config.opttolerance);
                path.fcurve = path.ocurve.clone();
            } else {
                path.fcurve = path.curve.clone();
            }
        }
    }

    paths
}

fn optimize_traced_vecpaths(paths: Vec<VecPath>, config: &TraceConfig) -> Vec<VecPath> {
    paths
        .into_iter()
        .map(|path| optimize_vecpath(path, config))
        .collect()
}

fn optimize_traced_vecpaths_with_cancel<F>(
    paths: Vec<VecPath>,
    config: &TraceConfig,
    should_cancel: &F,
) -> Option<Vec<VecPath>>
where
    F: Fn() -> bool,
{
    let mut optimized = Vec::with_capacity(paths.len());
    for path in paths {
        if should_cancel() {
            return None;
        }
        optimized.push(optimize_vecpath_with_cancel(path, config, should_cancel)?);
    }
    Some(optimized)
}

fn optimize_vecpath(path: VecPath, config: &TraceConfig) -> VecPath {
    let subpaths = path
        .subpaths
        .into_iter()
        .filter_map(|subpath| optimize_subpath(subpath, config))
        .collect();
    VecPath { subpaths }
}

fn optimize_vecpath_with_cancel<F>(
    path: VecPath,
    config: &TraceConfig,
    should_cancel: &F,
) -> Option<VecPath>
where
    F: Fn() -> bool,
{
    let mut subpaths = Vec::with_capacity(path.subpaths.len());
    for subpath in path.subpaths {
        if should_cancel() {
            return None;
        }
        if let Some(optimized) = optimize_subpath_with_cancel(subpath, config, should_cancel) {
            subpaths.push(optimized);
        }
    }
    Some(VecPath { subpaths })
}

fn optimize_subpath(subpath: SubPath, config: &TraceConfig) -> Option<SubPath> {
    let mut segments = extract_segments(&subpath);
    segments.retain(|segment| !is_degenerate_segment(segment));
    if segments.is_empty() {
        return None;
    }

    let mut optimized = Vec::new();
    let mut index = 0usize;
    while index < segments.len() {
        if let Some((replacement, consumed)) = find_best_run_replacement(&segments, index, config) {
            optimized.push(replacement);
            index += consumed;
        } else {
            optimized.push(segments[index].clone());
            index += 1;
        }
    }

    build_subpath_from_segments(&optimized, subpath.closed)
}

fn optimize_subpath_with_cancel<F>(
    subpath: SubPath,
    config: &TraceConfig,
    should_cancel: &F,
) -> Option<SubPath>
where
    F: Fn() -> bool,
{
    let mut segments = extract_segments(&subpath);
    segments.retain(|segment| !is_degenerate_segment(segment));
    if segments.is_empty() {
        return None;
    }

    let mut optimized = Vec::new();
    let mut index = 0usize;
    while index < segments.len() {
        if should_cancel() {
            return None;
        }
        if let Some((replacement, consumed)) = find_best_run_replacement(&segments, index, config) {
            optimized.push(replacement);
            index += consumed;
        } else {
            optimized.push(segments[index].clone());
            index += 1;
        }
    }

    build_subpath_from_segments(&optimized, subpath.closed)
}

fn extract_segments(subpath: &SubPath) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut current = Point2D::zero();
    let mut start = None;

    for command in &subpath.commands {
        match *command {
            PathCommand::MoveTo { x, y } => {
                current = Point2D::new(x, y);
                start = Some(current);
            }
            PathCommand::LineTo { .. }
            | PathCommand::QuadTo { .. }
            | PathCommand::CubicTo { .. } => {
                segments.push(Segment {
                    start: current,
                    command: *command,
                });
                current = segment_end(current, command);
            }
            PathCommand::Close => {
                if let Some(start_point) = start
                    && current.distance_to(&start_point) > EPSILON
                {
                    segments.push(Segment {
                        start: current,
                        command: PathCommand::LineTo {
                            x: start_point.x,
                            y: start_point.y,
                        },
                    });
                    current = start_point;
                }
            }
        }
    }

    segments
}

fn build_subpath_from_segments(segments: &[Segment], closed: bool) -> Option<SubPath> {
    let first = segments.first()?;
    let mut commands = Vec::with_capacity(segments.len() + 2);
    commands.push(PathCommand::MoveTo {
        x: first.start.x,
        y: first.start.y,
    });
    commands.extend(segments.iter().map(|segment| segment.command));
    if closed {
        commands.push(PathCommand::Close);
    }
    Some(SubPath { commands, closed })
}

fn find_best_run_replacement(
    segments: &[Segment],
    start_index: usize,
    config: &TraceConfig,
) -> Option<(Segment, usize)> {
    let max_len = config
        .optimizer_run_limit
        .min(segments.len().saturating_sub(start_index));
    if max_len < 2 {
        return None;
    }

    for run_len in (2..=max_len).rev() {
        let run = &segments[start_index..start_index + run_len];
        if run_has_sharp_corner(run, config.corner_preservation_angle_deg) {
            continue;
        }

        if let Some(candidate) = try_merge_run_as_line(run, config) {
            return Some((candidate, run_len));
        }

        if let Some(candidate) = try_merge_run_as_cubic(run, config) {
            return Some((candidate, run_len));
        }
    }

    None
}

fn try_merge_run_as_line(run: &[Segment], config: &TraceConfig) -> Option<Segment> {
    let samples = sample_run_points(run, 10);
    let start = run.first()?.start;
    let end = run.last()?.end();
    if start.distance_to(&end) <= EPSILON {
        return None;
    }

    let candidate = vec![start, end];
    let stats = polyline_error_stats(&samples, &candidate);
    if stats.max <= config.post_fit_max_deviation * 0.8 && stats.rms <= config.post_fit_rms_target {
        Some(Segment {
            start,
            command: PathCommand::LineTo { x: end.x, y: end.y },
        })
    } else {
        None
    }
}

fn try_merge_run_as_cubic(run: &[Segment], config: &TraceConfig) -> Option<Segment> {
    let start = run.first()?.start;
    let end = run.last()?.end();
    if start.distance_to(&end) <= EPSILON {
        return None;
    }

    let start_tangent = tangent_or_chord(start_tangent_of(run.first()?), start, end);
    let end_tangent = tangent_or_chord(end_tangent_of(run.last()?), start, end);
    if start_tangent.distance_to(&Point2D::zero()) <= EPSILON
        || end_tangent.distance_to(&Point2D::zero()) <= EPSILON
    {
        return None;
    }

    let run_samples = sample_run_points(run, 12);
    let chord = start.distance_to(&end);
    let arc = polyline_length(&run_samples);
    let base_start = estimate_handle_length(run.first()?, chord, arc, true);
    let base_end = estimate_handle_length(run.last()?, chord, arc, false);

    let mut best: Option<(PathCommand, ErrorStats)> = None;
    for start_scale in CUBIC_HANDLE_SCALES {
        for end_scale in CUBIC_HANDLE_SCALES {
            let handle_1 = scale_vector(start_tangent, base_start * start_scale);
            let handle_2 = scale_vector(end_tangent, base_end * end_scale);
            let command = PathCommand::CubicTo {
                c1x: start.x + handle_1.x,
                c1y: start.y + handle_1.y,
                c2x: end.x - handle_2.x,
                c2y: end.y - handle_2.y,
                x: end.x,
                y: end.y,
            };
            let candidate_samples = sample_segment_points(&Segment { start, command }, 24);
            let stats = bidirectional_error_stats(&run_samples, &candidate_samples);
            if stats.max <= config.post_fit_max_deviation
                && stats.rms <= config.post_fit_rms_target
                && best.is_none_or(|(_, current)| {
                    stats.max < current.max - 0.02
                        || ((stats.max - current.max).abs() <= 0.02 && stats.rms < current.rms)
                })
            {
                best = Some((command, stats));
            }
        }
    }

    best.map(|(command, _)| Segment { start, command })
}

fn run_has_sharp_corner(run: &[Segment], threshold_deg: f64) -> bool {
    run.windows(2).any(|pair| {
        let a = end_tangent_of(&pair[0]);
        let b = start_tangent_of(&pair[1]);
        angle_between(a, b) > threshold_deg
    })
}

fn sample_run_points(run: &[Segment], steps_per_segment: usize) -> Vec<Point2D> {
    let mut points = Vec::new();
    for (index, segment) in run.iter().enumerate() {
        let samples = sample_segment_points(segment, steps_per_segment);
        if index == 0 {
            points.extend(samples);
        } else {
            points.extend(samples.into_iter().skip(1));
        }
    }
    points
}

fn sample_segment_points(segment: &Segment, steps: usize) -> Vec<Point2D> {
    let start = segment.start;
    match segment.command {
        PathCommand::LineTo { x, y } => vec![start, Point2D::new(x, y)],
        PathCommand::QuadTo { cx, cy, x, y } => {
            let control = Point2D::new(cx, cy);
            let end = Point2D::new(x, y);
            (0..=steps)
                .map(|idx| {
                    let t = idx as f64 / steps as f64;
                    sample_quad(start, control, end, t)
                })
                .collect()
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let c1 = Point2D::new(c1x, c1y);
            let c2 = Point2D::new(c2x, c2y);
            let end = Point2D::new(x, y);
            (0..=steps)
                .map(|idx| {
                    let t = idx as f64 / steps as f64;
                    sample_cubic(start, c1, c2, end, t)
                })
                .collect()
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => vec![start],
    }
}

fn sample_quad(p0: Point2D, p1: Point2D, p2: Point2D, t: f64) -> Point2D {
    let mt = 1.0 - t;
    Point2D::new(
        mt * mt * p0.x + 2.0 * mt * t * p1.x + t * t * p2.x,
        mt * mt * p0.y + 2.0 * mt * t * p1.y + t * t * p2.y,
    )
}

fn sample_cubic(p0: Point2D, p1: Point2D, p2: Point2D, p3: Point2D, t: f64) -> Point2D {
    let mt = 1.0 - t;
    Point2D::new(
        mt * mt * mt * p0.x + 3.0 * mt * mt * t * p1.x + 3.0 * mt * t * t * p2.x + t * t * t * p3.x,
        mt * mt * mt * p0.y + 3.0 * mt * mt * t * p1.y + 3.0 * mt * t * t * p2.y + t * t * t * p3.y,
    )
}

fn start_tangent_of(segment: &Segment) -> Point2D {
    match segment.command {
        PathCommand::LineTo { x, y } => {
            normalize(Point2D::new(x - segment.start.x, y - segment.start.y))
        }
        PathCommand::QuadTo { cx, cy, x, y } => {
            let direct = Point2D::new(cx - segment.start.x, cy - segment.start.y);
            tangent_or_fallback(direct, segment.start, Point2D::new(x, y))
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let direct = Point2D::new(c1x - segment.start.x, c1y - segment.start.y);
            if magnitude(direct) > EPSILON {
                normalize(direct)
            } else {
                tangent_or_fallback(
                    Point2D::new(c2x - segment.start.x, c2y - segment.start.y),
                    segment.start,
                    Point2D::new(x, y),
                )
            }
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => Point2D::zero(),
    }
}

fn end_tangent_of(segment: &Segment) -> Point2D {
    let end = segment.end();
    match segment.command {
        PathCommand::LineTo { .. } => normalize(Point2D::new(
            end.x - segment.start.x,
            end.y - segment.start.y,
        )),
        PathCommand::QuadTo { cx, cy, .. } => {
            let direct = Point2D::new(end.x - cx, end.y - cy);
            tangent_or_fallback(direct, segment.start, end)
        }
        PathCommand::CubicTo { c2x, c2y, .. } => {
            let direct = Point2D::new(end.x - c2x, end.y - c2y);
            tangent_or_fallback(direct, segment.start, end)
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => Point2D::zero(),
    }
}

fn tangent_or_fallback(candidate: Point2D, start: Point2D, end: Point2D) -> Point2D {
    if magnitude(candidate) > EPSILON {
        normalize(candidate)
    } else {
        normalize(Point2D::new(end.x - start.x, end.y - start.y))
    }
}

fn tangent_or_chord(candidate: Point2D, start: Point2D, end: Point2D) -> Point2D {
    if magnitude(candidate) > EPSILON {
        candidate
    } else {
        normalize(Point2D::new(end.x - start.x, end.y - start.y))
    }
}

fn estimate_handle_length(segment: &Segment, chord: f64, arc: f64, from_start: bool) -> f64 {
    let default_length = (arc * 0.28).max(chord * 0.18).min(chord * 0.75 + 1.0);
    let estimated = match segment.command {
        PathCommand::LineTo { x, y } => {
            let end = Point2D::new(x, y);
            segment.start.distance_to(&end) * (1.0 / 3.0)
        }
        PathCommand::QuadTo { cx, cy, x, y } => {
            let control = Point2D::new(cx, cy);
            if from_start {
                segment.start.distance_to(&control)
            } else {
                control.distance_to(&Point2D::new(x, y))
            }
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            if from_start {
                segment.start.distance_to(&Point2D::new(c1x, c1y))
            } else {
                Point2D::new(x, y).distance_to(&Point2D::new(c2x, c2y))
            }
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => default_length,
    };

    estimated
        .max(default_length * 0.75)
        .min(default_length * 2.0 + 0.5)
}

fn polyline_error_stats(reference: &[Point2D], candidate: &[Point2D]) -> ErrorStats {
    if reference.is_empty() || candidate.len() < 2 {
        return ErrorStats::default();
    }

    let mut max_error: f64 = 0.0;
    let mut sum_sq = 0.0;
    for point in reference {
        let distance = point_to_polyline_distance(*point, candidate);
        max_error = max_error.max(distance);
        sum_sq += distance * distance;
    }

    ErrorStats {
        max: max_error,
        rms: (sum_sq / reference.len() as f64).sqrt(),
    }
}

fn bidirectional_error_stats(reference: &[Point2D], candidate: &[Point2D]) -> ErrorStats {
    let forward = polyline_error_stats(reference, candidate);
    let reverse = polyline_error_stats(candidate, reference);
    let total_samples = (reference.len() + candidate.len()).max(1) as f64;
    let rms = (((forward.rms * forward.rms) * reference.len() as f64)
        + ((reverse.rms * reverse.rms) * candidate.len() as f64))
        / total_samples;

    ErrorStats {
        max: forward.max.max(reverse.max),
        rms: rms.sqrt(),
    }
}

fn compute_path_set_error(before: &[VecPath], after: &[VecPath]) -> ErrorStats {
    let mut max_error: f64 = 0.0;
    let mut sum_sq = 0.0;
    let mut count = 0usize;

    for (before_path, after_path) in before.iter().zip(after.iter()) {
        let before_polys = flatten_vecpath(before_path, DEFAULT_TOLERANCE_MM / 2.0);
        let after_polys = flatten_vecpath(after_path, DEFAULT_TOLERANCE_MM / 2.0);
        for (before_poly, after_poly) in before_polys.iter().zip(after_polys.iter()) {
            let stats = bidirectional_error_stats(&before_poly.points, &after_poly.points);
            max_error = max_error.max(stats.max);
            let sample_count = before_poly.points.len() + after_poly.points.len();
            sum_sq += stats.rms * stats.rms * sample_count as f64;
            count += sample_count;
        }
    }

    ErrorStats {
        max: max_error,
        rms: if count == 0 {
            0.0
        } else {
            (sum_sq / count as f64).sqrt()
        },
    }
}

fn point_to_polyline_distance(point: Point2D, polyline: &[Point2D]) -> f64 {
    polyline
        .windows(2)
        .map(|segment| point_to_segment_distance(point, segment[0], segment[1]))
        .fold(f64::INFINITY, f64::min)
}

fn point_to_segment_distance(point: Point2D, a: Point2D, b: Point2D) -> f64 {
    let ab = Point2D::new(b.x - a.x, b.y - a.y);
    let ap = Point2D::new(point.x - a.x, point.y - a.y);
    let len_sq = ab.x * ab.x + ab.y * ab.y;
    if len_sq <= EPSILON {
        return point.distance_to(&a);
    }

    let t = ((ap.x * ab.x + ap.y * ab.y) / len_sq).clamp(0.0, 1.0);
    let projection = Point2D::new(a.x + ab.x * t, a.y + ab.y * t);
    point.distance_to(&projection)
}

fn polyline_length(points: &[Point2D]) -> f64 {
    points
        .windows(2)
        .map(|pair| pair[0].distance_to(&pair[1]))
        .sum()
}

fn is_degenerate_segment(segment: &Segment) -> bool {
    let end = segment.end();
    if segment.start.distance_to(&end) > 0.08 {
        return false;
    }

    match segment.command {
        PathCommand::LineTo { .. } => true,
        PathCommand::QuadTo { cx, cy, .. } => {
            segment.start.distance_to(&Point2D::new(cx, cy)) <= 0.08
                && end.distance_to(&Point2D::new(cx, cy)) <= 0.08
        }
        PathCommand::CubicTo {
            c1x, c1y, c2x, c2y, ..
        } => {
            let c1 = Point2D::new(c1x, c1y);
            let c2 = Point2D::new(c2x, c2y);
            segment.start.distance_to(&c1) <= 0.08
                && end.distance_to(&c2) <= 0.08
                && c1.distance_to(&c2) <= 0.12
        }
        PathCommand::MoveTo { .. } | PathCommand::Close => false,
    }
}

fn segment_end(start: Point2D, command: &PathCommand) -> Point2D {
    match *command {
        PathCommand::LineTo { x, y }
        | PathCommand::QuadTo { x, y, .. }
        | PathCommand::CubicTo { x, y, .. }
        | PathCommand::MoveTo { x, y } => Point2D::new(x, y),
        PathCommand::Close => start,
    }
}

fn angle_between(a: Point2D, b: Point2D) -> f64 {
    let ma = magnitude(a);
    let mb = magnitude(b);
    if ma <= EPSILON || mb <= EPSILON {
        return 0.0;
    }
    let dot = ((a.x * b.x + a.y * b.y) / (ma * mb)).clamp(-1.0, 1.0);
    dot.acos().to_degrees()
}

fn magnitude(point: Point2D) -> f64 {
    (point.x * point.x + point.y * point.y).sqrt()
}

fn normalize(point: Point2D) -> Point2D {
    let length = magnitude(point);
    if length <= EPSILON {
        Point2D::zero()
    } else {
        Point2D::new(point.x / length, point.y / length)
    }
}

fn scale_vector(point: Point2D, scale: f64) -> Point2D {
    Point2D::new(point.x * scale, point.y * scale)
}

fn effective_supersample_factor(width: u32, height: u32, configured: u32) -> u32 {
    if configured <= 1 {
        1
    } else if width.max(height) > 1400 {
        1
    } else {
        configured
    }
}

fn grayscale_with_optional_invert(image: &GrayImage, invert: bool) -> GrayImage {
    if !invert {
        return image.clone();
    }

    let (width, height) = image.dimensions();
    let mut out = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            out.put_pixel(x, y, Luma([255 - image.get_pixel(x, y)[0]]));
        }
    }
    out
}

fn local_normalize(image: &GrayImage, radius: u32, blend: f64) -> GrayImage {
    let (width, height) = image.dimensions();
    let mut out = GrayImage::new(width, height);
    let blend = blend.clamp(0.0, 1.0);
    let integral = integral_image(image);
    let target_mean = 160.0;
    let gain = 1.35;

    for y in 0..height {
        for x in 0..width {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(width);
            let y1 = (y + radius + 1).min(height);
            let area = ((x1 - x0) * (y1 - y0)).max(1) as f64;
            let local_mean = rect_sum(&integral, width, x0, y0, x1, y1) as f64 / area;
            let current = image.get_pixel(x, y)[0] as f64;
            let normalized = (target_mean + (current - local_mean) * gain).clamp(0.0, 255.0);
            let blended = current * (1.0 - blend) + normalized * blend;
            out.put_pixel(x, y, Luma([blended.clamp(0.0, 255.0) as u8]));
        }
    }

    out
}

fn integral_image(image: &GrayImage) -> Vec<u64> {
    let (width, height) = image.dimensions();
    let stride = (width + 1) as usize;
    let mut integral = vec![0u64; stride * (height as usize + 1)];

    for y in 0..height {
        let mut row_sum = 0u64;
        for x in 0..width {
            row_sum += image.get_pixel(x, y)[0] as u64;
            let idx = (y as usize + 1) * stride + (x as usize + 1);
            integral[idx] = integral[idx - stride] + row_sum;
        }
    }

    integral
}

fn rect_sum(integral: &[u64], image_width: u32, x0: u32, y0: u32, x1: u32, y1: u32) -> u64 {
    let stride = (image_width + 1) as usize;
    integral[y1 as usize * stride + x1 as usize] + integral[y0 as usize * stride + x0 as usize]
        - integral[y0 as usize * stride + x1 as usize]
        - integral[y1 as usize * stride + x0 as usize]
}

fn box_blur(image: &GrayImage, radius: u32) -> GrayImage {
    if radius == 0 {
        return image.clone();
    }

    let (width, height) = image.dimensions();
    let mut out = GrayImage::new(width, height);
    let integral = integral_image(image);
    let stride = (width + 1) as usize;

    for y in 0..height {
        for x in 0..width {
            let x0 = x.saturating_sub(radius);
            let y0 = y.saturating_sub(radius);
            let x1 = (x + radius + 1).min(width);
            let y1 = (y + radius + 1).min(height);
            let area = ((x1 - x0) * (y1 - y0)).max(1) as u64;
            let sum = integral[y1 as usize * stride + x1 as usize]
                + integral[y0 as usize * stride + x0 as usize]
                - integral[y0 as usize * stride + x1 as usize]
                - integral[y1 as usize * stride + x0 as usize];
            out.put_pixel(x, y, Luma([(sum / area) as u8]));
        }
    }

    out
}

fn downsample_mean(image: &GrayImage, factor: u32, width: u32, height: u32) -> GrayImage {
    let mut out = GrayImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let mut sum = 0u32;
            let mut count = 0u32;
            for sy in 0..factor {
                for sx in 0..factor {
                    let px = x * factor + sx;
                    let py = y * factor + sy;
                    sum += image.get_pixel(px, py)[0] as u32;
                    count += 1;
                }
            }
            out.put_pixel(x, y, Luma([(sum / count.max(1)) as u8]));
        }
    }
    out
}

fn despeckle_bitmap(bitmap: &mut [Vec<bool>], area_threshold: u32) {
    if bitmap.is_empty() || area_threshold == 0 {
        return;
    }

    remove_small_components(bitmap, true, area_threshold);
    remove_small_components(bitmap, false, area_threshold);
}

fn remove_small_components(bitmap: &mut [Vec<bool>], target_value: bool, area_threshold: u32) {
    let height = bitmap.len();
    let width = bitmap[0].len();
    let mut visited = vec![false; width * height];
    let neighbors = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)];

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            if visited[idx] || bitmap[y][x] != target_value {
                continue;
            }

            let mut stack = vec![(x, y)];
            let mut component = Vec::new();
            let mut touches_border = false;
            visited[idx] = true;

            while let Some((cx, cy)) = stack.pop() {
                component.push((cx, cy));
                touches_border |= cx == 0 || cy == 0 || cx + 1 == width || cy + 1 == height;

                for (dx, dy) in neighbors {
                    let nx = cx as i32 + dx;
                    let ny = cy as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    let nxi = nx as usize;
                    let nyi = ny as usize;
                    let nidx = nyi * width + nxi;
                    if !visited[nidx] && bitmap[nyi][nxi] == target_value {
                        visited[nidx] = true;
                        stack.push((nxi, nyi));
                    }
                }
            }

            if component.len() as u32 <= area_threshold && (target_value || !touches_border) {
                for (cx, cy) in component {
                    bitmap[cy][cx] = !target_value;
                }
            }
        }
    }
}

fn anchor_count(paths: &[VecPath]) -> usize {
    paths
        .iter()
        .flat_map(|path| &path.subpaths)
        .map(|subpath| {
            let has_move = subpath
                .commands
                .iter()
                .any(|command| matches!(command, PathCommand::MoveTo { .. }));
            let draws = subpath
                .commands
                .iter()
                .filter(|command| {
                    matches!(
                        command,
                        PathCommand::LineTo { .. }
                            | PathCommand::QuadTo { .. }
                            | PathCommand::CubicTo { .. }
                    )
                })
                .count();
            draws + usize::from(has_move)
        })
        .sum()
}
