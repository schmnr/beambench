//! Polygon scanline rasterizer for Fill layers.
//!
//! Converts closed vector polylines into a binary bitmap so that the existing
//! `generate_scanlines` pipeline can produce raster-style hatching for vector
//! shapes assigned to a Fill operation.

use beambench_common::geometry::Bounds;
use beambench_common::path::Polyline;
use beambench_raster::types::{ProcessedRaster, RasterPixelFormat};

/// Rasterize closed polylines into a binary bitmap using scanline fill.
///
/// Open polylines are skipped (they have no interior to fill).
/// Returns `None` if there are no closed polylines, bounds are degenerate,
/// or the resulting bitmap would be empty.
pub fn rasterize_fill(
    polylines: &[Polyline],
    bounds: &Bounds,
    line_interval_mm: f64,
) -> Option<ProcessedRaster> {
    if line_interval_mm <= 0.0 {
        return None;
    }

    let w = bounds.width();
    let h = bounds.height();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }

    // Collect edges from closed polylines only
    let mut edges: Vec<Edge> = Vec::new();
    for poly in polylines {
        if !poly.closed || poly.points.len() < 3 {
            continue;
        }
        let pts = &poly.points;
        let n = pts.len();
        for i in 0..n {
            let p0 = &pts[i];
            let p1 = &pts[(i + 1) % n];
            // Skip horizontal edges (they don't contribute intersections)
            if (p0.y - p1.y).abs() < 1e-9 {
                continue;
            }
            edges.push(Edge {
                y_min: p0.y.min(p1.y),
                y_max: p0.y.max(p1.y),
                x0: p0.x,
                y0: p0.y,
                x1: p1.x,
                y1: p1.y,
            });
        }
    }

    if edges.is_empty() {
        return None;
    }

    let width_px = ((w / line_interval_mm).ceil() as u32).max(1);
    let height_px = ((h / line_interval_mm).ceil() as u32).max(1);
    let row_bytes = (width_px as usize).div_ceil(8);
    // Initialize all bits to 1 (white/no-burn). We clear bits to 0 for burn pixels.
    let mut data = vec![0xFFu8; row_bytes * height_px as usize];

    for row in 0..height_px {
        let y_world = bounds.min.y + row as f64 * line_interval_mm + line_interval_mm * 0.5;

        // Collect intersections
        let mut intersections: Vec<f64> = Vec::new();
        for edge in &edges {
            if y_world < edge.y_min || y_world >= edge.y_max {
                continue;
            }
            // Linear interpolation: x = x0 + (y - y0) / (y1 - y0) * (x1 - x0)
            let t = (y_world - edge.y0) / (edge.y1 - edge.y0);
            let x = edge.x0 + t * (edge.x1 - edge.x0);
            intersections.push(x);
        }

        intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Fill between pairs (even-odd rule)
        let mut i = 0;
        while i + 1 < intersections.len() {
            let x_start = intersections[i];
            let x_end = intersections[i + 1];

            // Convert to pixel columns
            let col_start = ((x_start - bounds.min.x) / line_interval_mm).floor() as i64;
            let col_end = ((x_end - bounds.min.x) / line_interval_mm).ceil() as i64;

            let col_start = col_start.max(0) as u32;
            let col_end = (col_end as u32).min(width_px);

            for col in col_start..col_end {
                // Clear bit to 0 (burn pixel). Binary format: 0 = burn.
                let byte_idx = row as usize * row_bytes + col as usize / 8;
                let bit_idx = 7 - (col as usize % 8);
                if byte_idx < data.len() {
                    data[byte_idx] &= !(1 << bit_idx);
                }
            }

            i += 2;
        }
    }

    Some(ProcessedRaster {
        width_px,
        height_px,
        line_interval_mm,
        x_pixel_mm: line_interval_mm,
        format: RasterPixelFormat::Binary,
        data,
    })
}

struct Edge {
    y_min: f64,
    y_max: f64,
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::geometry::Point2D;

    fn make_unit_square() -> Vec<Polyline> {
        vec![Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
                Point2D::new(0.0, 10.0),
            ],
            true,
        )]
    }

    fn make_triangle() -> Vec<Polyline> {
        vec![Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(5.0, 10.0),
            ],
            true,
        )]
    }

    #[test]
    fn filled_unit_square_all_bits_set() {
        let polylines = make_unit_square();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let result = rasterize_fill(&polylines, &bounds, 1.0).unwrap();

        assert_eq!(result.width_px, 10);
        assert_eq!(result.height_px, 10);
        assert_eq!(result.format, RasterPixelFormat::Binary);

        // Every pixel should be a burn pixel (bit = 0).
        let row_bytes = (result.width_px as usize).div_ceil(8);
        for row in 0..result.height_px {
            for col in 0..result.width_px {
                let byte_idx = row as usize * row_bytes + col as usize / 8;
                let bit_idx = 7 - (col as usize % 8);
                let is_burn = (result.data[byte_idx] & (1 << bit_idx)) == 0;
                assert!(is_burn, "Pixel ({col}, {row}) should be burn but was white");
            }
        }
    }

    #[test]
    fn open_polyline_returns_none() {
        let polylines = vec![Polyline::new(
            vec![
                Point2D::new(0.0, 0.0),
                Point2D::new(10.0, 0.0),
                Point2D::new(10.0, 10.0),
            ],
            false, // open
        )];
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        assert!(rasterize_fill(&polylines, &bounds, 1.0).is_none());
    }

    #[test]
    fn empty_polylines_returns_none() {
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        assert!(rasterize_fill(&[], &bounds, 1.0).is_none());
    }

    #[test]
    fn triangle_has_partial_fill() {
        let polylines = make_triangle();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let result = rasterize_fill(&polylines, &bounds, 1.0).unwrap();

        assert_eq!(result.width_px, 10);
        assert_eq!(result.height_px, 10);

        // Count burn pixels — should be more than 0 but less than all
        let row_bytes = (result.width_px as usize).div_ceil(8);
        let mut burn_count = 0u32;
        for row in 0..result.height_px {
            for col in 0..result.width_px {
                let byte_idx = row as usize * row_bytes + col as usize / 8;
                let bit_idx = 7 - (col as usize % 8);
                if (result.data[byte_idx] & (1 << bit_idx)) == 0 {
                    burn_count += 1;
                }
            }
        }
        let total = result.width_px * result.height_px;
        assert!(burn_count > 0, "Triangle should have some burn pixels");
        assert!(burn_count < total, "Triangle should not fill entire bitmap");
    }

    #[test]
    fn degenerate_bounds_returns_none() {
        let polylines = make_unit_square();
        // Zero-width bounds
        let bounds = Bounds::new(Point2D::new(5.0, 0.0), Point2D::new(5.0, 10.0));
        assert!(rasterize_fill(&polylines, &bounds, 1.0).is_none());

        // Zero-height bounds
        let bounds = Bounds::new(Point2D::new(0.0, 5.0), Point2D::new(10.0, 5.0));
        assert!(rasterize_fill(&polylines, &bounds, 1.0).is_none());
    }

    #[test]
    fn zero_line_interval_returns_none() {
        let polylines = make_unit_square();
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        assert!(rasterize_fill(&polylines, &bounds, 0.0).is_none());
    }

    #[test]
    fn two_separate_squares_both_filled() {
        // Two non-overlapping closed squares within bounds
        let polylines = vec![
            Polyline::new(
                vec![
                    Point2D::new(1.0, 1.0),
                    Point2D::new(4.0, 1.0),
                    Point2D::new(4.0, 4.0),
                    Point2D::new(1.0, 4.0),
                ],
                true,
            ),
            Polyline::new(
                vec![
                    Point2D::new(6.0, 6.0),
                    Point2D::new(9.0, 6.0),
                    Point2D::new(9.0, 9.0),
                    Point2D::new(6.0, 9.0),
                ],
                true,
            ),
        ];
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let result = rasterize_fill(&polylines, &bounds, 1.0).unwrap();

        let row_bytes = (result.width_px as usize).div_ceil(8);
        let mut burn_count = 0u32;
        for row in 0..result.height_px {
            for col in 0..result.width_px {
                let byte_idx = row as usize * row_bytes + col as usize / 8;
                let bit_idx = 7 - (col as usize % 8);
                if (result.data[byte_idx] & (1 << bit_idx)) == 0 {
                    burn_count += 1;
                }
            }
        }
        let total = result.width_px * result.height_px;
        assert!(burn_count > 0, "Should have burn pixels from two squares");
        assert!(
            burn_count < total,
            "Should not fill entire bitmap (two separate squares)"
        );
    }

    #[test]
    fn square_with_hole_even_odd() {
        // Outer square 0-10, inner square 3-7 → even-odd fill should leave hole
        let polylines = vec![
            // Outer square
            Polyline::new(
                vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(10.0, 10.0),
                    Point2D::new(0.0, 10.0),
                ],
                true,
            ),
            // Inner square (hole)
            Polyline::new(
                vec![
                    Point2D::new(3.0, 3.0),
                    Point2D::new(7.0, 3.0),
                    Point2D::new(7.0, 7.0),
                    Point2D::new(3.0, 7.0),
                ],
                true,
            ),
        ];
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let result = rasterize_fill(&polylines, &bounds, 1.0).unwrap();

        let row_bytes = (result.width_px as usize).div_ceil(8);

        // Check center pixel (5, 5) — should be INSIDE hole (white/no-burn)
        let center_row = 5u32;
        let center_col = 5u32;
        let byte_idx = center_row as usize * row_bytes + center_col as usize / 8;
        let bit_idx = 7 - (center_col as usize % 8);
        let center_is_burn = (result.data[byte_idx] & (1 << bit_idx)) == 0;
        assert!(
            !center_is_burn,
            "Center pixel should NOT be burn (it's inside the hole)"
        );

        // Check edge pixel (1, 1) — should be OUTSIDE hole (burn)
        let edge_row = 1u32;
        let edge_col = 1u32;
        let byte_idx = edge_row as usize * row_bytes + edge_col as usize / 8;
        let bit_idx = 7 - (edge_col as usize % 8);
        let edge_is_burn = (result.data[byte_idx] & (1 << bit_idx)) == 0;
        assert!(
            edge_is_burn,
            "Edge pixel should be burn (between outer and inner squares)"
        );
    }

    #[test]
    fn mixed_open_and_closed_only_fills_closed() {
        let polylines = vec![
            // Open polyline (should be ignored)
            Polyline::new(
                vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(5.0, 0.0),
                    Point2D::new(5.0, 5.0),
                ],
                false,
            ),
            // Closed polyline (should be filled)
            Polyline::new(
                vec![
                    Point2D::new(0.0, 0.0),
                    Point2D::new(10.0, 0.0),
                    Point2D::new(10.0, 10.0),
                    Point2D::new(0.0, 10.0),
                ],
                true,
            ),
        ];
        let bounds = Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0));
        let result = rasterize_fill(&polylines, &bounds, 1.0);
        assert!(result.is_some());
    }
}
