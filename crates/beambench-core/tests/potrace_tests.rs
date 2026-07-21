use beambench_common::path::PathCommand;
use beambench_core::trace::{
    TraceConfig, TraceProcessingBucket, trace_image, trace_image_with_report,
};
use image::{GrayImage, Luma};

fn white_canvas(width: u32, height: u32) -> GrayImage {
    GrayImage::from_pixel(width, height, Luma([255u8]))
}

#[test]
fn trace_solid_square_produces_closed_bezier_paths() {
    let mut img = white_canvas(20, 20);
    for y in 5..15 {
        for x in 5..15 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }

    let config = TraceConfig::default();
    let paths = trace_image(&img, &config);

    assert!(!paths.is_empty(), "should produce at least one path");
    for path in &paths {
        assert!(!path.subpaths.is_empty());
        assert!(path.subpaths[0].closed);
        let has_draw = path.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::LineTo { .. } | PathCommand::CubicTo { .. }));
        assert!(has_draw, "trace should produce drawable path commands");
    }
}

#[test]
fn trace_ring_produces_single_compound_vecpath_with_hole() {
    let mut img = white_canvas(30, 30);
    for y in 3..27 {
        for x in 3..27 {
            if !(x >= 10 && x < 20 && y >= 10 && y < 20) {
                img.put_pixel(x, y, image::Luma([0u8]));
            }
        }
    }

    let config = TraceConfig {
        turdsize: 1,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert_eq!(
        paths.len(),
        1,
        "ring should be one compound path, got {}",
        paths.len()
    );
    assert_eq!(
        paths[0].subpaths.len(),
        2,
        "compound path should have outer + hole subpaths, got {}",
        paths[0].subpaths.len()
    );
    assert!(paths[0].subpaths[0].closed);
    assert!(paths[0].subpaths[1].closed);
}

#[test]
fn trace_two_separate_squares_produces_two_vecpaths() {
    let mut img = white_canvas(40, 40);
    for y in 2..8 {
        for x in 2..8 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }
    for y in 25..35 {
        for x in 25..35 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }

    let config = TraceConfig {
        turdsize: 1,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert_eq!(
        paths.len(),
        2,
        "two disjoint squares should produce 2 VecPaths, got {}",
        paths.len()
    );
    assert_eq!(paths[0].subpaths.len(), 1);
    assert_eq!(paths[1].subpaths.len(), 1);
}

#[test]
fn trace_empty_image_produces_no_paths() {
    let img = white_canvas(10, 10);
    let paths = trace_image(&img, &TraceConfig::default());
    assert!(paths.is_empty());
}

#[test]
fn trace_turdsize_filters_small_features() {
    let mut img = white_canvas(30, 30);
    // Large square (area >> turdsize)
    for y in 5..15 {
        for x in 5..15 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }
    // Single pixel (area 1 < turdsize 5)
    img.put_pixel(25, 25, image::Luma([0u8]));

    let config = TraceConfig {
        turdsize: 5,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert_eq!(
        paths.len(),
        1,
        "single pixel should be filtered, only large square remains"
    );
}

#[test]
fn trace_alphamax_zero_produces_all_corners() {
    let mut img = white_canvas(20, 20);
    for y in 5..15 {
        for x in 5..15 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }

    let config = TraceConfig {
        alphamax: 0.0,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert!(!paths.is_empty());
    for path in &paths {
        let has_cubic = path.subpaths[0]
            .commands
            .iter()
            .any(|c| matches!(c, PathCommand::CubicTo { .. }));
        assert!(
            !has_cubic,
            "alphamax=0 should produce only corners (no CubicTo)"
        );
    }
}

#[test]
fn trace_opticurve_disabled_still_works() {
    let mut img = white_canvas(20, 20);
    for y in 5..15 {
        for x in 5..15 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }

    let config = TraceConfig {
        opticurve: false,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert!(!paths.is_empty());
    assert!(paths[0].subpaths[0].closed);
}

#[test]
fn trace_cutoff_excludes_pixels_below_range() {
    let mut img = white_canvas(30, 30);
    // Dim square (brightness 100)
    for y in 3..10 {
        for x in 3..10 {
            img.put_pixel(x, y, image::Luma([100u8]));
        }
    }
    // Bright square (brightness 200)
    for y in 15..25 {
        for x in 15..25 {
            img.put_pixel(x, y, image::Luma([200u8]));
        }
    }

    // cutoff=0 threshold=150 → only darker square traces.
    let config = TraceConfig {
        cutoff: 0,
        threshold: 150,
        turdsize: 1,
        ..Default::default()
    };
    let paths = trace_image(&img, &config);
    assert_eq!(paths.len(), 1, "only the darker square should be traced");
}

#[test]
fn trace_invert_flips_foreground() {
    let mut img = white_canvas(20, 20);
    // Light square on white background: no visible contour until invert flips it dark.
    for y in 5..15 {
        for x in 5..15 {
            img.put_pixel(x, y, image::Luma([220u8]));
        }
    }

    // Without invert: light square remains near background and should not trace.
    let config_normal = TraceConfig {
        turdsize: 1,
        ..Default::default()
    };
    let paths_normal = trace_image(&img, &config_normal);
    assert!(
        paths_normal.is_empty(),
        "light square should not trace without invert"
    );

    // With invert: light square becomes dark against a darkened background and should trace.
    let config_invert = TraceConfig {
        turdsize: 1,
        invert: true,
        ..Default::default()
    };
    let paths_invert = trace_image(&img, &config_invert);
    assert!(
        !paths_invert.is_empty(),
        "light square should trace with invert"
    );
}

#[test]
fn trace_sketch_trace_reduces_noise() {
    let mut img = white_canvas(30, 30);
    // Solid square
    for y in 8..22 {
        for x in 8..22 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }
    // Isolated noise pixels
    img.put_pixel(3, 3, image::Luma([60u8]));
    img.put_pixel(5, 27, image::Luma([70u8]));

    let config_no = TraceConfig {
        turdsize: 0,
        ..Default::default()
    };
    let paths_no = trace_image(&img, &config_no);

    let config_yes = TraceConfig {
        turdsize: 0,
        sketch_trace: true,
        ..Default::default()
    };
    let paths_yes = trace_image(&img, &config_yes);

    assert!(
        paths_yes.len() <= paths_no.len(),
        "sketch trace should reduce noise: {} vs {}",
        paths_yes.len(),
        paths_no.len()
    );
}

#[test]
fn trace_sketch_trace_handles_uneven_lighting_better() {
    let mut img = GrayImage::new(60, 30);
    for y in 0..30 {
        for x in 0..60 {
            // Background darkens from left to right, simulating uneven lighting.
            let bg = 255u8.saturating_sub((x * 2) as u8);
            img.put_pixel(x, y, image::Luma([bg]));
        }
    }
    // Add a dark vertical stroke across the image.
    for y in 4..26 {
        for x in 26..31 {
            img.put_pixel(x, y, image::Luma([20u8]));
        }
    }

    let plain = trace_image(
        &img,
        &TraceConfig {
            threshold: 128,
            cutoff: 0,
            turdsize: 1,
            ..Default::default()
        },
    );
    let sketch = trace_image(
        &img,
        &TraceConfig {
            threshold: 128,
            cutoff: 0,
            turdsize: 1,
            sketch_trace: true,
            ..Default::default()
        },
    );

    let plain_max_x = plain
        .iter()
        .filter_map(|p| p.visual_bounds())
        .map(|b| b.max.x)
        .fold(0.0f64, f64::max);
    let sketch_max_x = sketch
        .iter()
        .filter_map(|p| p.visual_bounds())
        .map(|b| b.max.x)
        .fold(0.0f64, f64::max);

    assert!(
        plain_max_x <= 36.0,
        "baseline preprocessing should already avoid tracing far into the darkened background, got max_x={plain_max_x}"
    );
    assert!(
        sketch_max_x <= plain_max_x + 0.5,
        "sketch trace should stay at least as focused as the default pipeline, got plain={plain_max_x} sketch={sketch_max_x}"
    );
}

#[test]
fn trace_report_reduces_nodes_on_soft_circle() {
    let mut img = white_canvas(96, 96);
    let center_x = 48.0;
    let center_y = 48.0;
    let radius = 26.0;

    for y in 0..96 {
        for x in 0..96 {
            let dx = x as f64 - center_x;
            let dy = y as f64 - center_y;
            let distance = (dx * dx + dy * dy).sqrt();
            let pixel = if distance <= radius - 1.0 {
                0u8
            } else if distance <= radius + 1.0 {
                (((distance - (radius - 1.0)) / 2.0) * 255.0).clamp(0.0, 255.0) as u8
            } else {
                255u8
            };
            img.put_pixel(x, y, image::Luma([pixel]));
        }
    }

    let report = trace_image_with_report(&img, &TraceConfig::default());
    assert!(!report.paths.is_empty(), "soft circle should trace");
    assert!(
        report.benchmark.anchor_count_after < report.benchmark.anchor_count_before,
        "optimizer should reduce anchors: before={} after={}",
        report.benchmark.anchor_count_before,
        report.benchmark.anchor_count_after
    );
    assert!(
        report.benchmark.max_deviation <= TraceConfig::default().post_fit_max_deviation + 0.2,
        "optimizer drift should stay bounded, got {}",
        report.benchmark.max_deviation
    );
}

#[test]
fn trace_report_exposes_reasonable_processing_bucket() {
    let mut img = white_canvas(48, 48);
    for y in 10..38 {
        for x in 20..28 {
            img.put_pixel(x, y, image::Luma([0u8]));
        }
    }

    let report = trace_image_with_report(&img, &TraceConfig::default());
    assert!(matches!(
        report.benchmark.processing_bucket,
        TraceProcessingBucket::Interactive
            | TraceProcessingBucket::Moderate
            | TraceProcessingBucket::Heavy
    ));
}
