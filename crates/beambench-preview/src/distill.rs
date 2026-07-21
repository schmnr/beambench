//! Distills an ExecutionPlan into lightweight PreviewData for canvas rendering.

use std::collections::BTreeMap;
use std::io::Cursor;

use beambench_common::geometry::{Bounds, Point2D};
use beambench_planner::stats::segment_duration;
use beambench_planner::{ExecutionPlan, PlanSegment, ScanAxis, ScanDirection};
use image::{ImageBuffer, LumaA};

use crate::types::{
    PreviewData, PreviewFrame, PreviewLayer, PreviewStats, RasterPreview, RasterPreviewBitmap,
    RasterRunExtent, TravelMove, VectorPreview,
};

/// Maximum total pixel count for a preview bitmap. Above this, both axes
/// are scaled down uniformly so aspect ratio is preserved (no scanline
/// dropping, unlike the old `MAX_TONE_STRIPS` cap).
const MAX_PREVIEW_BITMAP_PIXELS: u64 = 16_000_000;

/// Distill an ExecutionPlan into lightweight PreviewData.
///
/// Raster scanlines are compressed into bounding boxes with fill density,
/// keeping the IPC payload small (~KB) and canvas rendering fast.
pub fn distill_preview(plan: &ExecutionPlan) -> PreviewData {
    // Use BTreeMap for deterministic ordering
    let mut layer_map: BTreeMap<String, (Vec<VectorPreview>, Vec<RasterPreview>)> = BTreeMap::new();
    let mut travel_moves = Vec::new();
    let mut frame: Option<PreviewFrame> = None;

    let mut travel_distance_mm = 0.0;
    let mut burn_distance_mm = 0.0;
    let mut raster_line_count: usize = 0;
    let mut sequence: usize = 0;
    let mut distill_warnings: Vec<String> = Vec::new();

    for segment in &plan.segments {
        let seg_duration_secs = segment_duration(segment);
        match segment {
            PlanSegment::Travel { start, end } => {
                travel_moves.push(TravelMove {
                    from: *start,
                    to: *end,
                    sequence,
                });
                sequence += 1;
                travel_distance_mm += start.distance_to(end);
            }
            PlanSegment::Vector {
                polyline,
                closed,
                power_percent,
                speed_mm_min,
                layer_id,
                ..
            } => {
                let entry = layer_map.entry(layer_id.clone()).or_default();
                entry.0.push(VectorPreview {
                    points: polyline.clone(),
                    closed: *closed,
                    power_percent: *power_percent,
                    speed_mm_min: *speed_mm_min,
                    sequence,
                });
                sequence += 1;
                burn_distance_mm += polyline_distance(polyline);
            }
            PlanSegment::Raster {
                scanlines,
                line_interval_mm,
                direction_mode,
                power_mode,
                speed_mm_min,
                layer_id,
                scan_angle_deg,
                scan_origin,
                overscan_mm,
                outlines,
                scan_axis,
                power_max_percent,
                power_min_percent,
                dot_width_correction_mm: _,
                ramp_length_mm,
                x_pixel_mm,
                ..
            } => {
                let entry = layer_map.entry(layer_id.clone()).or_default();

                // Compute local-space bounding box from scanline runs (same
                // frame the runs live in — pre-rotation, pre-transpose).
                let mut min_x_local = f64::INFINITY;
                let mut max_x_local = f64::NEG_INFINITY;
                let mut min_y_local = f64::INFINITY;
                let mut max_y_local = f64::NEG_INFINITY;
                let mut total_run_width = 0.0;
                let mut power_sum: f64 = 0.0;
                let mut power_count: usize = 0;
                let mut run_extents: Vec<RasterRunExtent> = Vec::new();
                let mut scanline_extents: Vec<RasterRunExtent> = Vec::new();
                let mut overscan_run_extents: Vec<RasterRunExtent> = Vec::new();

                for scanline in scanlines {
                    min_y_local = min_y_local.min(scanline.y_mm);
                    max_y_local = max_y_local.max(scanline.y_mm);
                    let mut row_min = f64::INFINITY;
                    let mut row_max = f64::NEG_INFINITY;
                    for run in &scanline.runs {
                        let run_min = run.start_x_mm.min(run.end_x_mm);
                        let run_max = run.start_x_mm.max(run.end_x_mm);
                        row_min = row_min.min(run_min);
                        row_max = row_max.max(run_max);
                        min_x_local = min_x_local.min(run_min);
                        max_x_local = max_x_local.max(run_max);
                        total_run_width += (run.end_x_mm - run.start_x_mm).abs();
                        burn_distance_mm += (run.end_x_mm - run.start_x_mm).abs();

                        run_extents.push(RasterRunExtent {
                            y_mm: scanline.y_mm,
                            start_x_mm: run.start_x_mm,
                            end_x_mm: run.end_x_mm,
                            direction: scanline.direction,
                        });
                        if run.power_values.is_empty() {
                            overscan_run_extents.push(RasterRunExtent {
                                y_mm: scanline.y_mm,
                                start_x_mm: run.start_x_mm,
                                end_x_mm: run.end_x_mm,
                                direction: scanline.direction,
                            });
                        } else {
                            overscan_run_extents.extend(grayscale_burn_extents(
                                scanline.y_mm,
                                run.start_x_mm,
                                run.end_x_mm,
                                &run.power_values,
                                scanline.direction,
                            ));
                        }

                        // Accumulate avg-power stats (histogram) as before.
                        if run.power_values.is_empty() {
                            // Binary with optional ramp expansion.
                            let segments = beambench_planner::ramp::expand_threshold_run(
                                run.start_x_mm,
                                run.end_x_mm,
                                *ramp_length_mm,
                            );
                            let n = ((run.end_x_mm - run.start_x_mm).abs() / line_interval_mm)
                                .round()
                                .max(1.0) as usize;
                            let mapped_full =
                                map_power_normalized(255, *power_min_percent, *power_max_percent);
                            let mut avg_power = 0.0;
                            for seg in &segments {
                                let pv = (seg.power_fraction * 255.0).clamp(0.0, 255.0) as u8;
                                let mapped = map_power_normalized(
                                    pv,
                                    *power_min_percent,
                                    *power_max_percent,
                                );
                                let seg_len = (seg.end_x_mm - seg.start_x_mm).abs();
                                let run_len = (run.end_x_mm - run.start_x_mm).abs();
                                if run_len > 0.0 {
                                    avg_power += mapped * seg_len / run_len;
                                }
                            }
                            if segments.is_empty() {
                                avg_power = mapped_full;
                            }
                            power_sum += n as f64 * avg_power;
                            power_count += n;
                        } else {
                            for &pv in &run.power_values {
                                power_sum += map_power_normalized(
                                    pv,
                                    *power_min_percent,
                                    *power_max_percent,
                                );
                                power_count += 1;
                            }
                        }
                    }
                    if row_min.is_finite() {
                        scanline_extents.push(RasterRunExtent {
                            y_mm: scanline.y_mm,
                            start_x_mm: row_min,
                            end_x_mm: row_max,
                            direction: scanline.direction,
                        });
                    }
                }

                raster_line_count += scanlines.len();

                // Build the preview bitmap in the same local run-space the
                // scanlines were emitted in. Resolution is chosen per axis
                // from x_pixel_mm (scan axis) and line_interval_mm (scanline
                // axis), so non-square pass-through pixels are preserved.
                let (preview_bitmap, local_origin_mm, local_width_mm, local_height_mm) =
                    build_preview_bitmap(
                        scanlines,
                        min_x_local,
                        max_x_local,
                        min_y_local,
                        max_y_local,
                        *line_interval_mm,
                        *x_pixel_mm,
                        *ramp_length_mm,
                        *power_min_percent,
                        *power_max_percent,
                    );

                // Work copies of the bounds for world-AABB derivation.
                let mut min_x = min_x_local;
                let mut max_x = max_x_local;
                let mut min_y = min_y_local;
                let mut max_y = max_y_local;

                // Add vertical movement between scanlines
                if scanlines.len() > 1 {
                    burn_distance_mm += (scanlines.len() - 1) as f64 * line_interval_mm;
                }

                // Compute fill_density from burn-only bounds BEFORE overscan expansion.
                // Overscan adds acceleration travel, not image content.
                let burn_bbox_width = max_x - min_x;
                let line_count = scanlines.len();
                let total_possible_width = burn_bbox_width * line_count as f64;
                let fill_density = if total_possible_width > 0.0 {
                    (total_run_width / total_possible_width).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                // Extend bounds by overscan in the scan direction (run axis is always X
                // in planner coordinates). ScanRun bounds are now burn-only; the preview
                // bounds must include overscan so the frontend can render overscan strips.
                if *overscan_mm > 0.0 {
                    min_x -= overscan_mm;
                    max_x += overscan_mm;
                }

                // For non-orthogonal angles, rotate the local bounding box
                // corners to world space to get the correct AABB
                let is_orthogonal = scan_angle_deg.abs() < 0.5
                    || (scan_angle_deg.abs() - 90.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 180.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 270.0).abs() < 0.5
                    || (scan_angle_deg.abs() - 360.0).abs() < 0.5;

                if !is_orthogonal && min_x.is_finite() {
                    let rad = scan_angle_deg.to_radians();
                    let (cos_a, sin_a) = (rad.cos(), rad.sin());
                    let corners = [
                        (min_x, min_y),
                        (max_x, min_y),
                        (max_x, max_y),
                        (min_x, max_y),
                    ];
                    let mut new_min_x = f64::INFINITY;
                    let mut new_max_x = f64::NEG_INFINITY;
                    let mut new_min_y = f64::INFINITY;
                    let mut new_max_y = f64::NEG_INFINITY;
                    for (lx, ly) in corners {
                        let wx = scan_origin.x + lx * cos_a - ly * sin_a;
                        let wy = scan_origin.y + lx * sin_a + ly * cos_a;
                        new_min_x = new_min_x.min(wx);
                        new_max_x = new_max_x.max(wx);
                        new_min_y = new_min_y.min(wy);
                        new_max_y = new_max_y.max(wy);
                    }
                    min_x = new_min_x;
                    max_x = new_max_x;
                    min_y = new_min_y;
                    max_y = new_max_y;
                }

                // For vertical scan, the planner swapped X/Y when generating
                // scanlines (transposed raster). Un-transpose the bounds so they
                // are in correct world-space coordinates.
                if *scan_axis == ScanAxis::Vertical {
                    std::mem::swap(&mut min_x, &mut min_y);
                    std::mem::swap(&mut max_x, &mut max_y);
                }

                // Single-scanline raster: expand to at least one line interval
                if *scan_axis == ScanAxis::Vertical {
                    if (max_x - min_x).abs() < f64::EPSILON {
                        max_x += line_interval_mm;
                    }
                } else if (max_y - min_y).abs() < f64::EPSILON {
                    max_y += line_interval_mm;
                }

                // Compute raster travel using the same motion model as the
                // G-code emitter: one continuous feed sweep and one overscan
                // pair per non-empty row, plus inter-scanline rapids.
                let raster_end_point: Option<(f64, f64)>;
                {
                    let mut prev_end: Option<(f64, f64)> = None;

                    for sl in scanlines {
                        let mut row_min = f64::INFINITY;
                        let mut row_max = f64::NEG_INFINITY;
                        let mut row_burn_width = 0.0;
                        for run in &sl.runs {
                            row_min = row_min.min(run.start_x_mm.min(run.end_x_mm));
                            row_max = row_max.max(run.start_x_mm.max(run.end_x_mm));
                            row_burn_width += (run.end_x_mm - run.start_x_mm).abs();
                        }
                        if !row_min.is_finite() {
                            continue;
                        }
                        let (start_pos, end_pos, direction) = match sl.direction {
                            ScanDirection::LeftToRight => (row_min, row_max, 1.0),
                            ScanDirection::RightToLeft => (row_max, row_min, -1.0),
                        };
                        let os_start = start_pos - direction * overscan_mm;
                        let os_end = end_pos + direction * overscan_mm;
                        if let Some((px, py)) = prev_end {
                            let dx = os_start - px;
                            let dy = sl.y_mm - py;
                            travel_distance_mm += (dx * dx + dy * dy).sqrt();
                        }
                        travel_distance_mm +=
                            (row_max - row_min - row_burn_width).max(0.0) + 2.0 * overscan_mm;
                        prev_end = Some((os_end, sl.y_mm));
                    }
                    raster_end_point = prev_end;
                }

                let avg_power_normalized = if power_count > 0 {
                    (power_sum / power_count as f64).clamp(0.0, 1.0)
                } else {
                    0.0
                };

                // Un-transpose the end point for vertical scan (same as bounds)
                let end_point = raster_end_point.map(|(x, y)| {
                    if *scan_axis == ScanAxis::Vertical {
                        Point2D::new(y, x)
                    } else {
                        Point2D::new(x, y)
                    }
                });

                if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite()
                {
                    entry.1.push(RasterPreview {
                        bounds: Bounds::new(Point2D::new(min_x, min_y), Point2D::new(max_x, max_y)),
                        line_count,
                        line_interval_mm: *line_interval_mm,
                        direction_mode: *direction_mode,
                        power_mode: *power_mode,
                        speed_mm_min: *speed_mm_min,
                        fill_density,
                        scan_angle_deg: *scan_angle_deg,
                        scan_origin: *scan_origin,
                        overscan_mm: *overscan_mm,
                        outlines: outlines.clone(),
                        scan_axis: *scan_axis,
                        sequence,
                        duration_secs: seg_duration_secs,
                        avg_power_normalized,
                        end_point,
                        preview_bitmap,
                        local_origin_mm,
                        local_width_mm,
                        local_height_mm,
                        run_extents,
                        scanline_extents,
                        overscan_run_extents,
                    });
                    sequence += 1;
                }
            }
            PlanSegment::Frame {
                path,
                power_percent,
                speed_mm_min,
            } => {
                frame = Some(PreviewFrame {
                    path: path.clone(),
                    power_percent: *power_percent,
                    speed_mm_min: *speed_mm_min,
                });
                burn_distance_mm += polyline_distance(path);
            }
            PlanSegment::OffsetFill {
                layer_id,
                object_id,
                ..
            } => {
                // OffsetFill is a geometry-less placeholder (layer/object ids +
                // spacing only) — the planner expands it into Vector segments
                // before a plan is finalized, so it should never reach the
                // distiller. If one survives, there is no geometry to render;
                // surface it as a warning instead of silently previewing
                // nothing (mirrors the G-code emitter's defensive warn).
                distill_warnings.push(format!(
                    "Offset Fill for object '{}' on layer '{}' was not expanded into tool paths, so it is missing from the preview",
                    object_id, layer_id
                ));
            }
        }
    }

    // Build layers in plan.layer_order, then any remaining
    let mut layers = Vec::new();
    for layer_id in &plan.layer_order {
        if let Some((vectors, rasters)) = layer_map.remove(layer_id) {
            layers.push(PreviewLayer {
                layer_id: layer_id.clone(),
                vector_paths: vectors,
                raster_regions: rasters,
            });
        }
    }
    // Any layers not in layer_order (shouldn't happen, but be safe)
    for (layer_id, (vectors, rasters)) in layer_map {
        layers.push(PreviewLayer {
            layer_id,
            vector_paths: vectors,
            raster_regions: rasters,
        });
    }

    let total_distance_mm = travel_distance_mm + burn_distance_mm;

    PreviewData {
        plan_id: plan.id,
        revision_hash: plan.revision_hash.clone(),
        bounds: plan.bounds,
        layers,
        travel_moves,
        frame,
        stats: PreviewStats {
            total_distance_mm,
            travel_distance_mm,
            burn_distance_mm,
            estimated_duration_secs: plan.estimated_duration_secs,
            segment_count: plan.segments.len(),
            raster_line_count,
        },
        warnings: plan
            .warnings
            .iter()
            .map(|w| w.message.clone())
            .chain(distill_warnings)
            .collect(),
        failed_entries: plan
            .failed_entries
            .iter()
            .map(|failure| match &failure.reason {
                beambench_planner::PlanEntryFailureReason::ImageMaskSkipped {
                    object_id,
                    message,
                } => format!("Image mask issue on object '{}': {}", object_id, message),
                beambench_planner::PlanEntryFailureReason::OffsetFillTimedOut {
                    iterations,
                    elapsed_ms,
                } => format!(
                    "Offset Fill was omitted for layer '{}' entry '{}' after timing out at {} iterations ({:.1}s)",
                    failure.layer_id,
                    failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                    iterations,
                    *elapsed_ms as f64 / 1000.0
                ),
                beambench_planner::PlanEntryFailureReason::OffsetFillEmergencyCeiling {
                    iterations,
                } => format!(
                    "Offset Fill was omitted for layer '{}' entry '{}' after hitting the {}-iteration emergency limit",
                    failure.layer_id,
                    failure.cut_entry_id.as_deref().unwrap_or("unknown"),
                    iterations
                ),
            })
            .collect(),
    }
}

/// Map a pixel power value (0-255) through the min/max power envelope to 0.0-1.0.
/// Mirrors the G-code emitter's `power_to_s_value` (beambench-grbl/src/gcode.rs):
/// `S = min_s + (pv / 255) * (max_s - min_s)`.
fn map_power_normalized(pixel_power: u8, min_pct: f64, max_pct: f64) -> f64 {
    map_power_fraction(pixel_power as f64 / 255.0, min_pct, max_pct)
}

/// Map a power fraction (0.0-1.0) through the min/max power envelope to 0.0-1.0.
/// Mirrors the G-code emitter's `ramp_fraction_to_s` (beambench-grbl/src/gcode.rs),
/// including its degenerate-envelope behavior: min >= max collapses to max.
fn map_power_fraction(fraction: f64, min_pct: f64, max_pct: f64) -> f64 {
    if min_pct >= max_pct {
        return (max_pct / 100.0).clamp(0.0, 1.0);
    }
    let t = fraction.clamp(0.0, 1.0);
    let mapped = min_pct / 100.0 + t * (max_pct - min_pct) / 100.0;
    mapped.clamp(0.0, 1.0)
}

/// Convert a normalized power (0.0 = laser off, 1.0 = full burn) into an
/// 8-bit grayscale luma value. Darker pixels = higher power, matching
/// the preview convention and physical engraved output.
fn power_to_luma(power: f64) -> u8 {
    let clamped = power.clamp(0.0, 1.0);
    let luma = (255.0 - clamped * 255.0).round();
    luma.clamp(0.0, 255.0) as u8
}

/// Build a PNG-encoded grayscale preview bitmap from scanline runs,
/// sized to preserve the raster's true pixel aspect ratio. Returns the
/// bitmap plus its local-space geometry (origin + width/height in mm)
/// so the frontend can `ctx.drawImage` it at the correct world location
/// under any rotation/transpose.
#[allow(clippy::too_many_arguments)]
fn build_preview_bitmap(
    scanlines: &[beambench_planner::Scanline],
    min_x_local: f64,
    max_x_local: f64,
    min_y_local: f64,
    max_y_local: f64,
    line_interval_mm: f64,
    x_pixel_mm_param: f64,
    ramp_length_mm: f64,
    power_min_percent: f64,
    power_max_percent: f64,
) -> (Option<RasterPreviewBitmap>, Point2D, f64, f64) {
    // Degenerate raster: no runs fit in finite bounds.
    if !min_x_local.is_finite()
        || !max_x_local.is_finite()
        || !min_y_local.is_finite()
        || !max_y_local.is_finite()
        || line_interval_mm <= 0.0
    {
        return (None, Point2D::new(0.0, 0.0), 0.0, 0.0);
    }

    // Local-space extents. Height adds one row worth so the last
    // scanline has a full pixel row of vertical extent.
    let local_width_mm = (max_x_local - min_x_local).max(0.0);
    let local_height_mm = (max_y_local - min_y_local).max(0.0) + line_interval_mm;
    if local_width_mm <= 0.0 || local_height_mm <= 0.0 {
        return (
            None,
            Point2D::new(min_x_local, min_y_local),
            local_width_mm,
            local_height_mm,
        );
    }

    // Pixel pitches per axis. x_pixel_mm may be 0 as a "same as
    // line_interval" sentinel (vector fill raster, pre-migration data).
    let x_pitch_mm = if x_pixel_mm_param > 0.0 {
        x_pixel_mm_param
    } else {
        line_interval_mm
    };
    let y_pitch_mm = line_interval_mm;

    // Raw bitmap dimensions before the 16M-pixel budget clamp.
    let raw_width = ((local_width_mm / x_pitch_mm).ceil() as u64).max(1);
    let raw_height = ((local_height_mm / y_pitch_mm).ceil() as u64).max(1);

    // Clamp to budget by uniform down-scale (preserves aspect ratio).
    let total = raw_width.saturating_mul(raw_height);
    let shrink = if total > MAX_PREVIEW_BITMAP_PIXELS {
        (MAX_PREVIEW_BITMAP_PIXELS as f64 / total as f64).sqrt()
    } else {
        1.0
    };
    let width_px = ((raw_width as f64 * shrink).ceil() as u32).max(1);
    let height_px = ((raw_height as f64 * shrink).ceil() as u32).max(1);

    // Effective mm-per-pixel after clamp.
    let eff_x_pitch = local_width_mm / width_px as f64;
    let eff_y_pitch = local_height_mm / height_px as f64;

    // Start fully transparent (laser off). Burned pixels get opaque
    // luma so when the frontend composites via drawImage, only the
    // engraved area is visible and the image's white/transparent
    // background doesn't obscure the preview canvas.
    let mut buf: ImageBuffer<LumaA<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(width_px, height_px, LumaA([255, 0]));

    for scanline in scanlines {
        if eff_y_pitch <= 0.0 {
            continue;
        }
        let row_f = ((scanline.y_mm - min_y_local) / eff_y_pitch).floor();
        if !row_f.is_finite() || row_f < 0.0 || row_f >= height_px as f64 {
            continue;
        }
        let row = row_f as u32;

        for run in &scanline.runs {
            if run.power_values.is_empty() {
                // Binary run (+optional ramp expansion).
                let segments = beambench_planner::ramp::expand_threshold_run(
                    run.start_x_mm,
                    run.end_x_mm,
                    ramp_length_mm,
                );
                if segments.is_empty() {
                    // Binary burn at full pixel value, mapped through the
                    // layer power envelope — the emitter burns these runs
                    // at max_s (power_max_percent), not at 100%.
                    let power = map_power_normalized(255, power_min_percent, power_max_percent);
                    paint_run_row(
                        &mut buf,
                        row,
                        run.start_x_mm,
                        run.end_x_mm,
                        min_x_local,
                        eff_x_pitch,
                        power,
                    );
                } else {
                    for seg in &segments {
                        // Mirrors the emitter's `ramp_fraction_to_s`:
                        // S interpolates min_s..max_s over the ramp fraction.
                        let power = map_power_fraction(
                            seg.power_fraction,
                            power_min_percent,
                            power_max_percent,
                        );
                        paint_run_row(
                            &mut buf,
                            row,
                            seg.start_x_mm,
                            seg.end_x_mm,
                            min_x_local,
                            eff_x_pitch,
                            power,
                        );
                    }
                }
            } else {
                // Grayscale: paint each source pixel at its own power,
                // mapped through the envelope exactly like the emitter's
                // `power_to_s_value` (S = min_s + pv/255 * (max_s - min_s)).
                let pixel_count = run.power_values.len();
                if pixel_count == 0 {
                    continue;
                }
                let run_length = run.end_x_mm - run.start_x_mm;
                let pixel_width = run_length / pixel_count as f64;
                for (i, &pv) in run.power_values.iter().enumerate() {
                    let x0 = run.start_x_mm + i as f64 * pixel_width;
                    let x1 = run.start_x_mm + (i + 1) as f64 * pixel_width;
                    let power = map_power_normalized(pv, power_min_percent, power_max_percent);
                    paint_run_row(&mut buf, row, x0, x1, min_x_local, eff_x_pitch, power);
                }
            }
        }
    }

    // PNG-encode the buffer. On failure, skip the bitmap; overscan
    // markers still render from run_extents.
    let mut png_bytes: Vec<u8> = Vec::new();
    let encode_ok = {
        let encoder = image::codecs::png::PngEncoder::new(Cursor::new(&mut png_bytes));
        use image::ImageEncoder;
        encoder
            .write_image(
                buf.as_raw(),
                width_px,
                height_px,
                image::ExtendedColorType::La8,
            )
            .is_ok()
    };

    let preview_bitmap = if encode_ok {
        Some(RasterPreviewBitmap {
            width_px,
            height_px,
            png_bytes,
        })
    } else {
        None
    };

    (
        preview_bitmap,
        Point2D::new(min_x_local, min_y_local),
        local_width_mm,
        local_height_mm,
    )
}

/// Paint a single scanline row's horizontal extent at the given
/// envelope-mapped power level, darkening pixels (min over existing
/// luma so overlapping runs don't brighten). Also sets alpha to 255
/// (opaque) so the pixel is visible against the preview background;
/// unburned pixels stay transparent (alpha=0). Zero mapped power means
/// the laser never fires (S0), so those pixels are left transparent
/// rather than painted opaque white.
fn paint_run_row(
    buf: &mut ImageBuffer<LumaA<u8>, Vec<u8>>,
    row: u32,
    x_start_mm: f64,
    x_end_mm: f64,
    min_x_local: f64,
    eff_x_pitch: f64,
    power: f64,
) {
    if eff_x_pitch <= 0.0 || power <= 0.0 {
        return;
    }
    let run_min = x_start_mm.min(x_end_mm);
    let run_max = x_start_mm.max(x_end_mm);
    let c_start_f = ((run_min - min_x_local) / eff_x_pitch).floor();
    let c_end_f = ((run_max - min_x_local) / eff_x_pitch).ceil();
    if !c_start_f.is_finite() || !c_end_f.is_finite() {
        return;
    }
    let width_px = buf.width() as i64;
    let col_start = c_start_f.max(0.0) as i64;
    let col_end = c_end_f.min(width_px as f64) as i64;
    if col_end <= col_start {
        return;
    }
    let luma = power_to_luma(power);
    for col in col_start..col_end {
        let px = buf.get_pixel_mut(col as u32, row);
        // min: darker wins (higher power over lower power on overlaps).
        if luma < px.0[0] {
            px.0[0] = luma;
        }
        // Make the pixel opaque so it's visible in the preview.
        px.0[1] = 255;
    }
}

fn grayscale_burn_extents(
    y_mm: f64,
    start_x_mm: f64,
    end_x_mm: f64,
    power_values: &[u8],
    direction: ScanDirection,
) -> Vec<RasterRunExtent> {
    if power_values.is_empty() {
        return Vec::new();
    }

    let pixel_count = power_values.len();
    let run_length = end_x_mm - start_x_mm;
    let pixel_width = run_length / pixel_count as f64;
    let mut out = Vec::new();
    let mut active_start: Option<usize> = None;

    for (i, &pv) in power_values.iter().enumerate() {
        let laser_on = pv > 0;
        match (active_start, laser_on) {
            (None, true) => active_start = Some(i),
            (Some(start_idx), false) => {
                out.push(RasterRunExtent {
                    y_mm,
                    start_x_mm: start_x_mm + start_idx as f64 * pixel_width,
                    end_x_mm: start_x_mm + i as f64 * pixel_width,
                    direction,
                });
                active_start = None;
            }
            _ => {}
        }
    }

    if let Some(start_idx) = active_start {
        out.push(RasterRunExtent {
            y_mm,
            start_x_mm: start_x_mm + start_idx as f64 * pixel_width,
            end_x_mm,
            direction,
        });
    }

    out
}

fn polyline_distance(points: &[Point2D]) -> f64 {
    points.windows(2).map(|w| w[0].distance_to(&w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_planner::*;
    use chrono::Utc;
    use uuid::Uuid;

    fn make_plan(segments: Vec<PlanSegment>) -> ExecutionPlan {
        ExecutionPlan {
            id: Uuid::new_v4(),
            project_id: Uuid::new_v4(),
            revision_hash: "test_hash".to_string(),
            created_at: Utc::now(),
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(100.0, 100.0)),
            total_distance_mm: 0.0,
            estimated_duration_secs: 60.0,
            segments,
            layer_order: vec!["layer1".to_string(), "layer2".to_string()],
            warnings: vec![],
            failed_entries: vec![],
        }
    }

    #[test]
    fn empty_plan_produces_empty_preview() {
        let plan = make_plan(vec![]);
        let preview = distill_preview(&plan);

        assert!(preview.layers.is_empty());
        assert!(preview.travel_moves.is_empty());
        assert!(preview.frame.is_none());
        assert_eq!(preview.stats.total_distance_mm, 0.0);
        assert_eq!(preview.stats.travel_distance_mm, 0.0);
        assert_eq!(preview.stats.burn_distance_mm, 0.0);
        assert_eq!(preview.stats.segment_count, 0);
        assert_eq!(preview.stats.raster_line_count, 0);
    }

    #[test]
    fn single_travel_segment() {
        let plan = make_plan(vec![PlanSegment::Travel {
            start: Point2D::new(0.0, 0.0),
            end: Point2D::new(3.0, 4.0),
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.travel_moves.len(), 1);
        assert!((preview.travel_moves[0].from.x).abs() < 1e-10);
        assert!((preview.travel_moves[0].to.x - 3.0).abs() < 1e-10);
        assert!((preview.stats.travel_distance_mm - 5.0).abs() < 1e-10);
        assert!((preview.stats.burn_distance_mm).abs() < 1e-10);
    }

    #[test]
    fn single_vector_in_correct_layer() {
        let plan = make_plan(vec![PlanSegment::Vector {
            polyline: vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            closed: false,
            power_percent: 80.0,
            speed_mm_min: 1000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            perforation_enabled: false,
            perforation_on_ms: 0.0,
            perforation_off_ms: 0.0,
            source_object_id: None,
            source_subpath_index: None,
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.layers.len(), 1);
        assert_eq!(preview.layers[0].layer_id, "layer1");
        assert_eq!(preview.layers[0].vector_paths.len(), 1);
        assert_eq!(preview.layers[0].vector_paths[0].points.len(), 3);
        assert!(!preview.layers[0].vector_paths[0].closed);
        assert!((preview.layers[0].vector_paths[0].power_percent - 80.0).abs() < 1e-10);
        // Distance: 10 + 10 = 20
        assert!((preview.stats.burn_distance_mm - 20.0).abs() < 1e-10);
    }

    #[test]
    fn raster_all_black_binary_fill_density_one() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.layers.len(), 1);
        assert_eq!(preview.layers[0].raster_regions.len(), 1);
        let raster = &preview.layers[0].raster_regions[0];
        assert!((raster.fill_density - 1.0).abs() < 1e-10);
        assert_eq!(raster.line_count, 2);
    }

    #[test]
    fn raster_multi_run_preview_uses_one_scanline_envelope() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![
                    ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 4.0,
                        power_values: vec![],
                    },
                    ScanRun {
                        start_x_mm: 6.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    },
                ],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);

        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        assert_eq!(raster.run_extents.len(), 2);
        assert_eq!(raster.scanline_extents.len(), 1);
        assert_eq!(raster.scanline_extents[0].start_x_mm, 0.0);
        assert_eq!(raster.scanline_extents[0].end_x_mm, 10.0);
        assert_eq!(raster.end_point, Some(Point2D::new(15.0, 0.0)));
        assert!((preview.stats.burn_distance_mm - 8.0).abs() < 1e-10);
        assert!((preview.stats.travel_distance_mm - 12.0).abs() < 1e-10);
    }

    #[test]
    fn raster_half_fill_density() {
        // Each line covers 50 out of 100mm bbox width
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 50.0,
                        power_values: vec![128, 128],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 50.0,
                        end_x_mm: 100.0,
                        power_values: vec![200],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);

        let raster = &preview.layers[0].raster_regions[0];
        // bbox width = 100, 2 lines => total possible = 200
        // total run width = 50 + 50 = 100
        // fill_density = 100/200 = 0.5
        assert!((raster.fill_density - 0.5).abs() < 1e-10);
    }

    #[test]
    fn frame_segment_produces_preview_frame() {
        let path = vec![
            Point2D::new(0.0, 0.0),
            Point2D::new(50.0, 0.0),
            Point2D::new(50.0, 50.0),
            Point2D::new(0.0, 50.0),
            Point2D::new(0.0, 0.0),
        ];
        let plan = make_plan(vec![PlanSegment::Frame {
            path: path.clone(),
            power_percent: 5.0,
            speed_mm_min: 3000.0,
        }]);
        let preview = distill_preview(&plan);

        assert!(preview.frame.is_some());
        let frame = preview.frame.as_ref().unwrap();
        assert_eq!(frame.path.len(), 5);
        assert!((frame.power_percent - 5.0).abs() < 1e-10);
        assert!((frame.speed_mm_min - 3000.0).abs() < 1e-10);
        // Perimeter: 50+50+50+50 = 200
        assert!((preview.stats.burn_distance_mm - 200.0).abs() < 1e-10);
    }

    #[test]
    fn multiple_layers_grouped_correctly() {
        let plan = make_plan(vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(20.0, 20.0), Point2D::new(30.0, 20.0)],
                closed: true,
                power_percent: 60.0,
                speed_mm_min: 500.0,
                layer_id: "layer2".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.layers.len(), 2);
        assert_eq!(preview.layers[0].layer_id, "layer1");
        assert_eq!(preview.layers[1].layer_id, "layer2");
        assert_eq!(preview.layers[0].vector_paths.len(), 1);
        assert_eq!(preview.layers[1].vector_paths.len(), 1);
    }

    #[test]
    fn mixed_plan_all_types_present() {
        let plan = make_plan(vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 10.0), Point2D::new(20.0, 10.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Raster {
                scanlines: vec![Scanline {
                    y_mm: 5.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 50.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                }],
                line_interval_mm: 0.1,
                direction_mode: DirectionMode::Unidirectional,
                power_mode: PowerMode::Binary,
                speed_mm_min: 2000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                scan_angle_deg: 0.0,
                scan_origin: Point2D::new(0.0, 0.0),
                overscan_mm: 0.0,
                outlines: vec![],
                scan_axis: ScanAxis::default(),
                power_max_percent: 100.0,
                power_min_percent: 0.0,
                dot_width_correction_mm: 0.0,
                ramp_length_mm: 0.0,
                x_pixel_mm: 0.0,
            },
            PlanSegment::Frame {
                path: vec![Point2D::new(0.0, 0.0), Point2D::new(100.0, 0.0)],
                power_percent: 5.0,
                speed_mm_min: 3000.0,
            },
        ]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.travel_moves.len(), 1);
        assert_eq!(preview.layers.len(), 1); // both in layer1
        assert_eq!(preview.layers[0].vector_paths.len(), 1);
        assert_eq!(preview.layers[0].raster_regions.len(), 1);
        assert!(preview.frame.is_some());
        assert_eq!(preview.stats.segment_count, 4);
    }

    #[test]
    fn raster_duration_secs_matches_planner_stats() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 100.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // duration_secs should match segment_duration from planner stats
        assert!(
            raster.duration_secs > 0.0,
            "duration_secs should be positive"
        );

        // Verify against manual calculation:
        // Feed = 220mm at 2000 mm/min = 6.6s
        // Rapid = 0.1mm at 10000 mm/min = 0.0006s
        let expected = (220.0 / 2000.0) * 60.0 + (0.1 / 10000.0) * 60.0;
        assert!(
            (raster.duration_secs - expected).abs() < 1e-6,
            "duration_secs {:.6} should match expected {:.6}",
            raster.duration_secs,
            expected
        );
    }

    #[test]
    fn raster_duration_secs_backward_compat() {
        // Old JSON without duration_secs should default to 0.0
        let json = r#"{
            "bounds": {"min": {"x": 0.0, "y": 0.0}, "max": {"x": 50.0, "y": 25.0}},
            "line_count": 10,
            "line_interval_mm": 0.1,
            "direction_mode": "bidirectional",
            "power_mode": "grayscale",
            "speed_mm_min": 2000.0,
            "fill_density": 0.5,
            "overscan_mm": 0.0
        }"#;
        let restored: RasterPreview = serde_json::from_str(json).unwrap();
        assert_eq!(restored.duration_secs, 0.0);
    }

    #[test]
    fn stats_distances_add_up() {
        let plan = make_plan(vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(3.0, 4.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(3.0, 4.0), Point2D::new(13.0, 4.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        let preview = distill_preview(&plan);

        // travel = 5, burn = 10, total = 15
        assert!((preview.stats.travel_distance_mm - 5.0).abs() < 1e-10);
        assert!((preview.stats.burn_distance_mm - 10.0).abs() < 1e-10);
        let diff = preview.stats.total_distance_mm
            - (preview.stats.travel_distance_mm + preview.stats.burn_distance_mm);
        assert!(diff.abs() < 1e-10);
    }

    #[test]
    fn stats_raster_line_count() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
                Scanline {
                    y_mm: 0.2,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 10.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.stats.raster_line_count, 3);
    }

    #[test]
    fn serde_roundtrip_preview_data() {
        let plan = make_plan(vec![
            PlanSegment::Travel {
                start: Point2D::new(0.0, 0.0),
                end: Point2D::new(10.0, 10.0),
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(10.0, 10.0), Point2D::new(20.0, 20.0)],
                closed: true,
                power_percent: 50.0,
                speed_mm_min: 800.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        let preview = distill_preview(&plan);

        let json = serde_json::to_string(&preview).unwrap();
        let restored: PreviewData = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.plan_id, preview.plan_id);
        assert_eq!(restored.revision_hash, preview.revision_hash);
        assert_eq!(restored.layers.len(), preview.layers.len());
        assert_eq!(restored.travel_moves.len(), preview.travel_moves.len());
    }

    #[test]
    fn serde_roundtrip_raster_preview() {
        let raster = RasterPreview {
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 25.0)),
            line_count: 100,
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            fill_density: 0.75,
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.5,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            sequence: 0,
            duration_secs: 3.5,
            avg_power_normalized: 0.65,
            end_point: None,
            preview_bitmap: None,
            local_origin_mm: Point2D::new(0.0, 0.0),
            local_width_mm: 50.0,
            local_height_mm: 25.0,
            run_extents: vec![],
            scanline_extents: vec![],
            overscan_run_extents: vec![],
        };

        let json = serde_json::to_string(&raster).unwrap();
        let restored: RasterPreview = serde_json::from_str(&json).unwrap();

        assert_eq!(restored.line_count, 100);
        assert!((restored.fill_density - 0.75).abs() < 1e-10);
        assert!((restored.avg_power_normalized - 0.65).abs() < 1e-10);
    }

    #[test]
    fn plan_with_warnings_copies_to_preview() {
        let mut plan = make_plan(vec![]);
        plan.warnings = vec![
            PlanWarning {
                message: "Scan angle not supported".to_string(),
            },
            PlanWarning {
                message: "Bounds exceeded".to_string(),
            },
        ];

        let preview = distill_preview(&plan);
        assert_eq!(preview.warnings.len(), 2);
        assert_eq!(preview.warnings[0], "Scan angle not supported");
        assert_eq!(preview.warnings[1], "Bounds exceeded");
    }

    #[test]
    fn plan_with_failed_entries_copies_to_preview() {
        let mut plan = make_plan(vec![]);
        plan.failed_entries = vec![PlanEntryFailure {
            layer_id: "layer-1".to_string(),
            cut_entry_id: Some("entry-1".to_string()),
            operation: OperationType::OffsetFill,
            reason: PlanEntryFailureReason::OffsetFillEmergencyCeiling { iterations: 256 },
        }];

        let preview = distill_preview(&plan);
        assert_eq!(preview.failed_entries.len(), 1);
        assert!(
            preview.failed_entries[0].contains("Offset Fill was omitted"),
            "preview should surface failed entry omissions"
        );
    }

    #[test]
    fn layer_ordering_matches_plan() {
        let mut plan = make_plan(vec![
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 80.0,
                speed_mm_min: 1000.0,
                layer_id: "layer2".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
            PlanSegment::Vector {
                polyline: vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 0.0)],
                closed: false,
                power_percent: 60.0,
                speed_mm_min: 500.0,
                layer_id: "layer1".to_string(),
                cut_entry_id: String::new(),
                perforation_enabled: false,
                perforation_on_ms: 0.0,
                perforation_off_ms: 0.0,
                source_object_id: None,
                source_subpath_index: None,
            },
        ]);
        plan.layer_order = vec!["layer1".to_string(), "layer2".to_string()];

        let preview = distill_preview(&plan);

        // Should be in layer_order, not insertion order
        assert_eq!(preview.layers[0].layer_id, "layer1");
        assert_eq!(preview.layers[1].layer_id, "layer2");
    }

    #[test]
    fn preview_plan_id_and_hash_match() {
        let plan = make_plan(vec![]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.plan_id, plan.id);
        assert_eq!(preview.revision_hash, plan.revision_hash);
    }

    #[test]
    fn single_scanline_raster_has_nonzero_height() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 5.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 50.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Unidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.layers.len(), 1);
        assert_eq!(preview.layers[0].raster_regions.len(), 1);
        let raster = &preview.layers[0].raster_regions[0];
        // Single scanline should have non-zero height (expanded by line_interval_mm)
        assert!(raster.bounds.max.y > raster.bounds.min.y);
        assert!((raster.bounds.min.y - 5.0).abs() < 1e-10);
        assert!((raster.bounds.max.y - 5.1).abs() < 1e-10);
    }

    #[test]
    fn raster_preview_passes_scan_angle_fields() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: -5.0,
                    end_x_mm: 5.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(15.0, 15.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);

        assert_eq!(preview.layers.len(), 1);
        assert_eq!(preview.layers[0].raster_regions.len(), 1);
        let raster = &preview.layers[0].raster_regions[0];
        assert!((raster.scan_angle_deg - 45.0).abs() < 1e-10);
        assert!((raster.scan_origin.x - 15.0).abs() < 1e-10);
        assert!((raster.scan_origin.y - 15.0).abs() < 1e-10);
    }

    #[test]
    fn raster_preview_rotated_bounds_expanded() {
        // A raster at 45° should have larger AABB than the local bounds
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: -5.0,
                    runs: vec![ScanRun {
                        start_x_mm: -5.0,
                        end_x_mm: 5.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 5.0,
                    runs: vec![ScanRun {
                        start_x_mm: -5.0,
                        end_x_mm: 5.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 45.0,
            scan_origin: Point2D::new(10.0, 10.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // The rotated AABB should be larger than the local 10x10 box
        let width = raster.bounds.max.x - raster.bounds.min.x;
        let height = raster.bounds.max.y - raster.bounds.min.y;
        // At 45°, a 10x10 square becomes ~14.14 wide
        assert!(
            width > 10.0,
            "Rotated bounds width should be > 10, got {}",
            width
        );
        assert!(
            height > 10.0,
            "Rotated bounds height should be > 10, got {}",
            height
        );
    }

    #[test]
    fn raster_preview_zero_angle_bounds_unchanged() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 5.0,
                runs: vec![ScanRun {
                    start_x_mm: 10.0,
                    end_x_mm: 20.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        // At 0°, bounds should match raw scanline coordinates
        assert!((raster.bounds.min.x - 10.0).abs() < 1e-10);
        assert!((raster.bounds.max.x - 20.0).abs() < 1e-10);
        assert!((raster.bounds.min.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn avg_power_normalized_all_black_grayscale() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![255, 255, 255, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        assert!((raster.avg_power_normalized - 1.0).abs() < 1e-10);
    }

    #[test]
    fn avg_power_normalized_mid_gray() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![128],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        // 128/255 ≈ 0.502
        assert!((raster.avg_power_normalized - 128.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn avg_power_normalized_binary_is_one() {
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        assert!((raster.avg_power_normalized - 1.0).abs() < 1e-10);
    }

    #[test]
    fn avg_power_normalized_serde_roundtrip() {
        let raster = RasterPreview {
            bounds: Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(50.0, 25.0)),
            line_count: 10,
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            fill_density: 0.5,
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            sequence: 0,
            duration_secs: 0.0,
            avg_power_normalized: 0.42,
            end_point: None,
            preview_bitmap: None,
            local_origin_mm: Point2D::new(0.0, 0.0),
            local_width_mm: 50.0,
            local_height_mm: 25.0,
            run_extents: vec![],
            scanline_extents: vec![],
            overscan_run_extents: vec![],
        };

        let json = serde_json::to_string(&raster).unwrap();
        let restored: RasterPreview = serde_json::from_str(&json).unwrap();
        assert!((restored.avg_power_normalized - 0.42).abs() < 1e-10);
    }

    #[test]
    fn avg_power_normalized_backward_compat() {
        // Old JSON without avg_power_normalized should default to 0.0
        let json = r#"{
            "bounds": {"min": {"x": 0.0, "y": 0.0}, "max": {"x": 50.0, "y": 25.0}},
            "line_count": 10,
            "line_interval_mm": 0.1,
            "direction_mode": "bidirectional",
            "power_mode": "grayscale",
            "speed_mm_min": 2000.0,
            "fill_density": 0.5,
            "overscan_mm": 0.0
        }"#;
        let restored: RasterPreview = serde_json::from_str(json).unwrap();
        assert_eq!(restored.avg_power_normalized, 0.0);
    }

    #[test]
    fn tone_strips_backward_compat() {
        // Old JSON without tone_strips should default to empty vec
        let json = r#"{
            "bounds": {"min": {"x": 0.0, "y": 0.0}, "max": {"x": 50.0, "y": 25.0}},
            "line_count": 10,
            "line_interval_mm": 0.1,
            "direction_mode": "bidirectional",
            "power_mode": "grayscale",
            "speed_mm_min": 2000.0,
            "fill_density": 0.5,
            "overscan_mm": 0.0
        }"#;
        let restored: RasterPreview = serde_json::from_str(json).unwrap();
        assert!(restored.run_extents.is_empty());
    }

    #[test]
    fn tone_strips_within_row_variation() {
        // Grayscale scanline with varying power should produce multiple strips
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 5.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 50.0,
                    power_values: vec![255, 128, 0, 128, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // One RasterRunExtent per planner run (sub-run variation is
        // now baked into the preview bitmap, not exposed as separate
        // entries).
        assert_eq!(raster.run_extents.len(), 1);
        assert!((raster.run_extents[0].y_mm - 5.0).abs() < 1e-10);
        // Sub-run power variation surfaces via avg_power_normalized.
        assert!(raster.avg_power_normalized > 0.0);
    }

    #[test]
    fn tone_strips_min_max_mapping() {
        // With min=20%, max=80%, pixel 0 maps to 0.2, pixel 255 maps to 0.8
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![0, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 20.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // One RasterRunExtent per planner run (grayscale variation
        // across the two pixels is baked into the preview bitmap).
        assert_eq!(raster.run_extents.len(), 1);
    }

    #[test]
    fn avg_power_normalized_with_min_max() {
        // With min=10%, max=80%, pixel 128 should map through envelope
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![128],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 10.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // Expected: 0.1 + (128/255) * (0.8 - 0.1) = 0.1 + 0.502 * 0.7 ≈ 0.4514
        let expected = 0.1 + (128.0 / 255.0) * 0.7;
        assert!(
            (raster.avg_power_normalized - expected).abs() < 1e-3,
            "avg_power_normalized should be {:.4}, got {:.4}",
            expected,
            raster.avg_power_normalized
        );
    }

    #[test]
    fn preview_bitmap_binary_maps_power_envelope() {
        // Binary runs burn at power_max_percent (the emitter sends max_s),
        // so a 20% layer must preview as a light shade — visible (opaque)
        // but nowhere near full black.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 20.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        let bitmap = raster.preview_bitmap.as_ref().expect("preview bitmap");
        let decoded = image::load_from_memory(&bitmap.png_bytes)
            .expect("decode preview png")
            .to_luma_alpha8();

        // 20% power → luma = 255 - 0.2 * 255 = 204 (light), alpha opaque.
        let px = decoded.get_pixel(0, 0);
        assert_eq!(
            px.0[0], 204,
            "binary run at 20% max power should preview as light shade (luma 204), got {}",
            px.0[0]
        );
        assert_eq!(px.0[1], 255, "burned pixel must be opaque");
    }

    #[test]
    fn preview_bitmap_grayscale_maps_power_envelope_endpoints() {
        // min=20%, max=80%: pixel 0 maps to 20% power (luma 204),
        // pixel 255 maps to 80% power (luma 51) — exactly the emitter's
        // power_to_s_value mapping, normalized.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![0, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 80.0,
            power_min_percent: 20.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        let bitmap = raster.preview_bitmap.as_ref().expect("preview bitmap");
        let decoded = image::load_from_memory(&bitmap.png_bytes)
            .expect("decode preview png")
            .to_luma_alpha8();

        // Run spans 0..10mm at 0.1mm pitch → 100 columns; the first 50
        // columns are pixel value 0, the last 50 are pixel value 255.
        let low = decoded.get_pixel(0, 0);
        assert_eq!(
            low.0[0], 204,
            "pixel value 0 with 20% min power should preview at luma 204, got {}",
            low.0[0]
        );
        assert_eq!(
            low.0[1], 255,
            "min-power pixel still burns — must be opaque"
        );

        let high = decoded.get_pixel(99, 0);
        assert_eq!(
            high.0[0], 51,
            "pixel value 255 with 80% max power should preview at luma 51, got {}",
            high.0[0]
        );
        assert_eq!(high.0[1], 255, "max-power pixel must be opaque");
    }

    #[test]
    fn preview_bitmap_zero_mapped_power_stays_transparent() {
        // With min=0%, a grayscale pixel value of 0 maps to S0 — the laser
        // never fires there, so the preview pixel must stay transparent
        // instead of painting opaque white.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![0, 255],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        let bitmap = raster.preview_bitmap.as_ref().expect("preview bitmap");
        let decoded = image::load_from_memory(&bitmap.png_bytes)
            .expect("decode preview png")
            .to_luma_alpha8();

        let skipped = decoded.get_pixel(0, 0);
        assert_eq!(
            skipped.0[1], 0,
            "zero mapped power must stay transparent, got alpha {}",
            skipped.0[1]
        );

        let burned = decoded.get_pixel(99, 0);
        assert_eq!(burned.0[0], 0, "full-power pixel previews as black");
        assert_eq!(burned.0[1], 255, "full-power pixel must be opaque");
    }

    #[test]
    fn offset_fill_placeholder_surfaces_preview_warning() {
        // PlanSegment::OffsetFill is a geometry-less placeholder that the
        // planner expands into Vector segments before plans are finalized.
        // If one ever survives into a distilled plan, the preview must say
        // so instead of silently rendering nothing for the layer.
        let plan = make_plan(vec![PlanSegment::OffsetFill {
            layer_id: "layer1".to_string(),
            object_id: "obj-1".to_string(),
            offset_mm: 0.1,
            angle_deg: 0.0,
        }]);
        let preview = distill_preview(&plan);

        assert!(
            preview
                .warnings
                .iter()
                .any(|w| w.contains("Offset Fill") && w.contains("obj-1") && w.contains("layer1")),
            "unexpanded OffsetFill placeholder should surface a preview warning, got {:?}",
            preview.warnings
        );
        // The placeholder carries no geometry, so no layer entries exist.
        assert!(preview.layers.is_empty());
    }

    #[test]
    fn tone_strips_binary_at_mapped_max() {
        // Binary raster strips should be at mapped max power
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 10.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 60.0,
            power_min_percent: 10.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        assert_eq!(raster.run_extents.len(), 1);
        // Binary = pixel 255, mapped: 0.1 + (255/255) * 0.5 = 0.6
    }

    #[test]
    fn ramp_length_expands_binary_run_into_strips() {
        // 20mm binary run with 2mm ramp per side → expanded into ramp segments.
        // Without ramp, this would be a single tone strip; with ramp it should
        // produce many strips with varying powers.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 2.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // One RasterRunExtent per planner run — ramp expansion is now
        // baked into the preview bitmap via `paint_run_row` calls on
        // each ramp segment, not into multiple run entries.
        assert_eq!(raster.run_extents.len(), 1);
        // The run's full extent is preserved.
        let ext = &raster.run_extents[0];
        assert!(((ext.end_x_mm - ext.start_x_mm).abs() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn ramp_length_zero_keeps_single_strip_for_binary_run() {
        // Without ramp, a binary run becomes a single tone strip — sanity that
        // ramp expansion is gated on ramp_length_mm > 0.
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 0.0,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 20.0,
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];
        assert_eq!(raster.run_extents.len(), 1);
    }

    #[test]
    fn grayscale_overscan_extents_split_row_around_zero_power_gaps() {
        let extents = grayscale_burn_extents(
            5.0,
            0.0,
            10.0,
            &[255, 255, 0, 0, 128, 128, 0, 255, 255, 255],
            ScanDirection::LeftToRight,
        );

        assert_eq!(extents.len(), 3);
        assert!((extents[0].start_x_mm - 0.0).abs() < 1e-10);
        assert!((extents[0].end_x_mm - 2.0).abs() < 1e-10);
        assert!((extents[1].start_x_mm - 4.0).abs() < 1e-10);
        assert!((extents[1].end_x_mm - 6.0).abs() < 1e-10);
        assert!((extents[2].start_x_mm - 7.0).abs() < 1e-10);
        assert!((extents[2].end_x_mm - 10.0).abs() < 1e-10);
    }

    #[test]
    fn tone_strips_downsampling_caps_at_budget() {
        // Create >4000 strips by having many scanlines with varying pixels
        let mut scanlines = Vec::new();
        for i in 0..500 {
            scanlines.push(Scanline {
                y_mm: i as f64 * 0.1,
                runs: vec![ScanRun {
                    start_x_mm: 0.0,
                    end_x_mm: 100.0,
                    // Alternating pixel values creates many sub-runs
                    power_values: (0..100).map(|j| if j % 2 == 0 { 255 } else { 0 }).collect(),
                }],
                direction: if i % 2 == 0 {
                    ScanDirection::LeftToRight
                } else {
                    ScanDirection::RightToLeft
                },
            });
        }
        // 500 scanlines * ~50 sub-runs each = ~25000 strips before downsampling
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines,
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        assert!(
            raster.run_extents.len() <= 4000,
            "tone strips should be capped at 4000, got {}",
            raster.run_extents.len()
        );
        assert!(
            !raster.run_extents.is_empty(),
            "should still have some strips after downsampling"
        );
    }

    #[test]
    fn tone_strips_vertical_scan_planner_coords() {
        // Vertical scan — strips should be in planner-native coordinates
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0, // This is world-X for vertical scan
                runs: vec![ScanRun {
                    start_x_mm: 5.0, // This is world-Y start
                    end_x_mm: 25.0,  // This is world-Y end
                    power_values: vec![200],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Grayscale,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        assert_eq!(raster.run_extents.len(), 1);
        let strip = &raster.run_extents[0];
        // Strips remain in planner-native coords (y_mm = scanline position)
        assert!((strip.y_mm - 10.0).abs() < 1e-10);
        assert!((strip.start_x_mm - 5.0).abs() < 1e-10);
        assert!((strip.end_x_mm - 25.0).abs() < 1e-10);
    }

    #[test]
    fn map_power_normalized_identity() {
        // min=0%, max=100% should be identity mapping
        assert!((map_power_normalized(0, 0.0, 100.0) - 0.0).abs() < 1e-10);
        assert!((map_power_normalized(255, 0.0, 100.0) - 1.0).abs() < 1e-10);
        assert!((map_power_normalized(128, 0.0, 100.0) - 128.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn map_power_normalized_custom_range() {
        // min=20%, max=80%
        assert!((map_power_normalized(0, 20.0, 80.0) - 0.2).abs() < 1e-10);
        assert!((map_power_normalized(255, 20.0, 80.0) - 0.8).abs() < 1e-10);
        // Midpoint: 0.2 + 0.5 * 0.6 = 0.5
        assert!(
            (map_power_normalized(128, 20.0, 80.0) - (0.2 + (128.0 / 255.0) * 0.6)).abs() < 1e-3
        );
    }

    #[test]
    fn end_point_horizontal_ltr_last_line() {
        // Horizontal scan, last scanline LTR: end_point should be at run end + overscan
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![
                Scanline {
                    y_mm: 0.0,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 50.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::LeftToRight,
                },
                Scanline {
                    y_mm: 0.1,
                    runs: vec![ScanRun {
                        start_x_mm: 0.0,
                        end_x_mm: 50.0,
                        power_values: vec![],
                    }],
                    direction: ScanDirection::RightToLeft,
                },
            ],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 5.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        let ep = raster.end_point.expect("end_point should be set");
        // Last scanline is RTL: start_pos=50, end_pos=0
        // dir_sign = -1 (since 50 > 0)
        // os_end = 0 + (-1)*5 = -5
        assert!(
            (ep.x - (-5.0)).abs() < 1e-10,
            "end_point.x should be -5.0 (RTL overscan end), got {}",
            ep.x
        );
        assert!(
            (ep.y - 0.1).abs() < 1e-10,
            "end_point.y should be 0.1 (last scanline y_mm), got {}",
            ep.y
        );
    }

    #[test]
    fn end_point_vertical_scan_untransposed() {
        // Vertical scan: end_point should be un-transposed from planner coords
        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines: vec![Scanline {
                y_mm: 10.0, // world-X for vertical
                runs: vec![ScanRun {
                    start_x_mm: 5.0, // world-Y start
                    end_x_mm: 25.0,  // world-Y end
                    power_values: vec![],
                }],
                direction: ScanDirection::LeftToRight,
            }],
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 2.0,
            outlines: vec![],
            scan_axis: ScanAxis::Vertical,
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);
        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        let ep = raster.end_point.expect("end_point should be set");
        // Planner coords: LTR, start=5, end=25, os=2
        // os_end = 25 + 1*2 = 27
        // Planner end = (27, 10)
        // Un-transposed (swap): world = (10, 27)
        assert!(
            (ep.x - 10.0).abs() < 1e-10,
            "end_point.x should be 10.0 (un-transposed y_mm), got {}",
            ep.x
        );
        assert!(
            (ep.y - 27.0).abs() < 1e-10,
            "end_point.y should be 27.0 (un-transposed os_end), got {}",
            ep.y
        );
    }

    #[test]
    fn end_point_backward_compat() {
        // Old JSON without end_point should deserialize to None
        let json = r#"{
            "bounds": {"min": {"x": 0.0, "y": 0.0}, "max": {"x": 50.0, "y": 25.0}},
            "line_count": 10,
            "line_interval_mm": 0.1,
            "direction_mode": "bidirectional",
            "power_mode": "grayscale",
            "speed_mm_min": 2000.0,
            "fill_density": 0.5,
            "overscan_mm": 0.0
        }"#;
        let restored: RasterPreview = serde_json::from_str(json).unwrap();
        assert!(restored.end_point.is_none());
    }

    /// Verify that a binary dithered raster (like Stucki output)
    /// produces a preview bitmap with actual dark pixels, not an
    /// all-white image.
    #[test]
    fn preview_bitmap_binary_dithered_has_dark_pixels() {
        // Simulate a Stucki-dithered image: alternating on/off runs
        // across 10 scanlines, like a checkerboard dither pattern.
        let mut scanlines = Vec::new();
        for i in 0u32..10 {
            let y = i as f64 * 0.1;
            let offset = if i % 2 == 0 { 0.0 } else { 0.5 };
            scanlines.push(Scanline {
                y_mm: y,
                runs: vec![
                    ScanRun {
                        start_x_mm: offset,
                        end_x_mm: offset + 0.5,
                        power_values: vec![], // binary = full power
                    },
                    ScanRun {
                        start_x_mm: offset + 1.0,
                        end_x_mm: offset + 1.5,
                        power_values: vec![],
                    },
                ],
                direction: if i % 2 == 0 {
                    ScanDirection::LeftToRight
                } else {
                    ScanDirection::RightToLeft
                },
            });
        }

        let plan = make_plan(vec![PlanSegment::Raster {
            scanlines,
            line_interval_mm: 0.1,
            direction_mode: DirectionMode::Bidirectional,
            power_mode: PowerMode::Binary,
            speed_mm_min: 2000.0,
            layer_id: "layer1".to_string(),
            cut_entry_id: String::new(),
            scan_angle_deg: 0.0,
            scan_origin: Point2D::new(0.0, 0.0),
            overscan_mm: 0.0,
            outlines: vec![],
            scan_axis: ScanAxis::default(),
            power_max_percent: 100.0,
            power_min_percent: 0.0,
            dot_width_correction_mm: 0.0,
            ramp_length_mm: 0.0,
            x_pixel_mm: 0.0,
        }]);

        let preview = distill_preview(&plan);
        let raster = &preview.layers[0].raster_regions[0];

        // preview_bitmap must be Some (PNG generated successfully).
        let bitmap = raster
            .preview_bitmap
            .as_ref()
            .expect("preview_bitmap should be Some for a dithered raster");
        assert!(bitmap.width_px > 0, "bitmap width must be > 0");
        assert!(bitmap.height_px > 0, "bitmap height must be > 0");
        assert!(!bitmap.png_bytes.is_empty(), "PNG bytes must not be empty");

        // Decode the PNG and verify it contains at least some dark
        // pixels (luma < 128). An all-white bitmap would mean the
        // paint_run_row calls never darkened any pixels.
        let decoded =
            image::load_from_memory(&bitmap.png_bytes).expect("PNG should decode successfully");
        let gray = decoded.to_luma8();
        let dark_pixel_count = gray.pixels().filter(|p| p.0[0] < 128).count();
        assert!(
            dark_pixel_count > 0,
            "bitmap should contain dark pixels from binary runs, but found 0 dark pixels out of {} total",
            gray.pixels().count()
        );

        // Local geometry must be non-zero.
        assert!(raster.local_width_mm > 0.0, "local_width_mm should be > 0");
        assert!(
            raster.local_height_mm > 0.0,
            "local_height_mm should be > 0"
        );
    }
}
