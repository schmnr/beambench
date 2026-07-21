//! Generates benchmark .lzrproj fixture files for planner testing.
//!
//! Run with:
//!   cargo nextest run -p beambench-planner --test generate_fixtures --run-ignored all
//! These fixtures are committed to the repo and loaded by other test modules.
//! The generator test is #[ignore]d because its output is nondeterministic
//! (fresh UUIDs/timestamps/version stamp); regenerate only when fixture
//! content should actually change, and commit the result deliberately.

use beambench_common::geometry::{Bounds, Point2D};
use beambench_common::{ColorTag, RasterAdjustments, RasterMode};
use beambench_core::asset::{Asset, AssetMediaType};
use beambench_core::layer::{Layer, OperationType};
use beambench_core::object::{ObjectData, ProjectObject, ShapeKind};
use beambench_core::project::Project;
use beambench_project::save_project;
use image::{GrayImage, ImageEncoder, Luma};
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/benchmarks")
}

fn encode_png(img: &GrayImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut bytes);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::L8,
        )
        .unwrap();
    bytes
}

/// 256x1 grayscale gradient (0..255).
fn make_gradient_image() -> (GrayImage, Vec<u8>) {
    let mut img = GrayImage::new(256, 1);
    for x in 0..256u32 {
        img.put_pixel(x, 0, Luma([x as u8]));
    }
    let bytes = encode_png(&img);
    (img, bytes)
}

/// 100x10 checkerboard pattern (alternating black/white pixels).
fn make_checkerboard_image() -> (GrayImage, Vec<u8>) {
    let mut img = GrayImage::new(100, 10);
    for y in 0..10u32 {
        for x in 0..100u32 {
            let val = if (x + y) % 2 == 0 { 255u8 } else { 0u8 };
            img.put_pixel(x, y, Luma([val]));
        }
    }
    let bytes = encode_png(&img);
    (img, bytes)
}

/// Fixture 1: Grayscale wedge — raster tonal fidelity test.
fn create_grayscale_wedge() -> Project {
    let mut project = Project::new("grayscale-wedge");

    let mut layer = Layer::new("Image Engrave", OperationType::Image);
    layer.color_tag = ColorTag("#FF0000".to_string());
    layer.primary_entry_mut().speed_mm_min = 6000.0;
    layer.primary_entry_mut().power_percent = 100.0;
    layer.primary_entry_mut().power_min_percent = 0.0;
    if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
        rs.dpi = 254;
        rs.mode = RasterMode::Grayscale;
    }
    let layer_id = layer.id;
    project.add_layer(layer);

    let (img, png_bytes) = make_gradient_image();
    let asset = Asset::new(
        "gradient.png",
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(img.width()),
        Some(img.height()),
    );
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    // Place offset from origin to allow overscan room, 50mm x 5mm
    let obj = ProjectObject::new(
        "gradient",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 15.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: img.width(),
            original_height_px: img.height(),
            adjustments: Some(RasterAdjustments::default()),
            masks: Vec::new(),
        },
    );
    project.add_object(obj);

    project
}

/// Fixture 2: Binary raster with overscan — tests overscan padding in G-code.
fn create_binary_raster_overscan() -> Project {
    let mut project = Project::new("binary-raster-overscan");

    let mut layer = Layer::new("Dithered Engrave", OperationType::Image);
    layer.color_tag = ColorTag("#00FF00".to_string());
    layer.primary_entry_mut().speed_mm_min = 6000.0;
    layer.primary_entry_mut().power_percent = 80.0;
    if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
        rs.dpi = 254;
        rs.mode = RasterMode::FloydSteinberg;
        rs.overscan_mm = 2.0;
    }
    let layer_id = layer.id;
    project.add_layer(layer);

    let (img, png_bytes) = make_checkerboard_image();
    let asset = Asset::new(
        "checkerboard.png",
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(img.width()),
        Some(img.height()),
    );
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    let obj = ProjectObject::new(
        "checkerboard",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(50.0, 14.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: img.width(),
            original_height_px: img.height(),
            adjustments: None,
            masks: Vec::new(),
        },
    );
    project.add_object(obj);

    project
}

/// Fixture 3: Dense short-segment vector — 50 small rectangles in 10x5 grid.
fn create_dense_short_segment_vector() -> Project {
    let mut project = Project::new("dense-short-segment-vector");

    let mut layer = Layer::new("Line Cut", OperationType::Line);
    layer.color_tag = ColorTag("#0000FF".to_string());
    layer.primary_entry_mut().speed_mm_min = 1000.0;
    layer.primary_entry_mut().power_percent = 60.0;
    let layer_id = layer.id;
    project.add_layer(layer);

    // 10 columns x 5 rows, 2mm x 2mm rectangles, 1mm gap
    for row in 0..5 {
        for col in 0..10 {
            let x = col as f64 * 3.0; // 2mm rect + 1mm gap
            let y = row as f64 * 3.0;
            let obj = ProjectObject::new(
                format!("rect_{row}_{col}"),
                layer_id,
                Bounds::new(Point2D::new(x, y), Point2D::new(x + 2.0, y + 2.0)),
                ObjectData::Shape {
                    kind: ShapeKind::Rectangle,
                    width: 2.0,
                    height: 2.0,
                    corner_radius: 0.0,
                },
            );
            project.add_object(obj);
        }
    }

    project
}

/// Fixture 4: Scan-angle fill — large rectangle with non-orthogonal fill.
fn create_scan_angle_fill() -> Project {
    let mut project = Project::new("scan-angle-fill");

    let mut layer = Layer::new("Fill Engrave", OperationType::Fill);
    layer.color_tag = ColorTag("#FF00FF".to_string());
    layer.primary_entry_mut().speed_mm_min = 3000.0;
    layer.primary_entry_mut().power_percent = 70.0;
    if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
        rs.line_interval_mm = 0.1;
        rs.scan_angle = 45.0;
        rs.dpi = 254;
    }
    let layer_id = layer.id;
    project.add_layer(layer);

    let obj = ProjectObject::new(
        "fill-rect",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(110.0, 60.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 100.0,
            height: 50.0,
            corner_radius: 0.0,
        },
    );
    project.add_object(obj);

    project
}

/// Fixture 5: Explicit angle-pass multipass fill — circle with 3 angled passes.
fn create_angle_pass_multipass() -> Project {
    let mut project = Project::new("angle-pass-multipass");

    let mut layer = Layer::new("Multipass Fill", OperationType::Fill);
    layer.color_tag = ColorTag("#FFFF00".to_string());
    layer.primary_entry_mut().speed_mm_min = 3000.0;
    layer.primary_entry_mut().power_percent = 50.0;
    if let Some(ref mut rs) = layer.primary_entry_mut().raster_settings {
        rs.crosshatch = false;
        rs.passes = 1;
        rs.line_interval_mm = 0.15;
        rs.dpi = 169; // ~25.4/0.15
        rs.scan_angle = 15.0;
        rs.angle_passes = 3;
        rs.angle_increment_deg = 45.0;
    }
    let layer_id = layer.id;
    project.add_layer(layer);

    // Circle (ellipse with equal width/height = diameter)
    let obj = ProjectObject::new(
        "circle",
        layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(60.0, 60.0)),
        ObjectData::Shape {
            kind: ShapeKind::Ellipse,
            width: 50.0,
            height: 50.0,
            corner_radius: 0.0,
        },
    );
    project.add_object(obj);

    project
}

/// Fixture 6: Origin placement — two layers, objects offset from (0,0).
fn create_fixture_origin_placement() -> Project {
    let mut project = Project::new("fixture-origin-placement");

    // Layer 1: Image at (10,10)
    let mut img_layer = Layer::new("Raster Layer", OperationType::Image);
    img_layer.color_tag = ColorTag("#FF8800".to_string());
    img_layer.primary_entry_mut().speed_mm_min = 4000.0;
    img_layer.primary_entry_mut().power_percent = 70.0;
    let img_layer_id = img_layer.id;
    project.add_layer(img_layer);

    // Small 10x10 solid white image
    let img = GrayImage::from_pixel(10, 10, Luma([200u8]));
    let png_bytes = encode_png(&img);
    let asset = Asset::new(
        "offset-image.png",
        AssetMediaType::Png,
        png_bytes.len() as u64,
        Some(10),
        Some(10),
    );
    let asset_key = asset.id.to_string();
    project.add_asset(asset, png_bytes);

    let raster_obj = ProjectObject::new(
        "offset-raster",
        img_layer_id,
        Bounds::new(Point2D::new(10.0, 10.0), Point2D::new(30.0, 30.0)),
        ObjectData::RasterImage {
            asset_key,
            original_width_px: 10,
            original_height_px: 10,
            adjustments: None,
            masks: Vec::new(),
        },
    );
    project.add_object(raster_obj);

    // Layer 2: Vector rect at (50,20)
    let mut vec_layer = Layer::new("Vector Layer", OperationType::Line);
    vec_layer.color_tag = ColorTag("#0088FF".to_string());
    vec_layer.primary_entry_mut().speed_mm_min = 2000.0;
    vec_layer.primary_entry_mut().power_percent = 60.0;
    let vec_layer_id = vec_layer.id;
    project.add_layer(vec_layer);

    let rect_obj = ProjectObject::new(
        "offset-rect",
        vec_layer_id,
        Bounds::new(Point2D::new(50.0, 20.0), Point2D::new(80.0, 40.0)),
        ObjectData::Shape {
            kind: ShapeKind::Rectangle,
            width: 30.0,
            height: 20.0,
            corner_radius: 0.0,
        },
    );
    project.add_object(rect_obj);

    project
}

// Ignored so routine suite runs do not rewrite the committed fixtures:
// generation is nondeterministic (fresh UUIDs, timestamps, and the current
// app version stamp), so every implicit run dirtied six .lzrproj files in
// the working tree with no semantic change.
#[test]
#[ignore = "regenerates committed fixtures; run deliberately via --run-ignored all"]
fn generate_benchmark_fixtures() {
    let dir = fixtures_dir();
    std::fs::create_dir_all(&dir).unwrap();

    let fixtures: Vec<(&str, Project)> = vec![
        ("grayscale-wedge", create_grayscale_wedge()),
        ("binary-raster-overscan", create_binary_raster_overscan()),
        (
            "dense-short-segment-vector",
            create_dense_short_segment_vector(),
        ),
        ("scan-angle-fill", create_scan_angle_fill()),
        ("angle-pass-multipass", create_angle_pass_multipass()),
        (
            "fixture-origin-placement",
            create_fixture_origin_placement(),
        ),
    ];

    for (name, project) in &fixtures {
        let path = dir.join(format!("{name}.lzrproj"));
        save_project(project, &path).unwrap_or_else(|e| panic!("Failed to save {name}: {e}"));
        assert!(path.exists(), "Fixture file not created: {name}");

        // Verify idempotent: re-load and check round-trip
        let loaded = beambench_project::load_project(&path)
            .unwrap_or_else(|e| panic!("Failed to load {name}: {e}"));
        assert_eq!(
            loaded.metadata.project_name, project.metadata.project_name,
            "Project name mismatch for {name}"
        );
        assert_eq!(
            loaded.layers.len(),
            project.layers.len(),
            "Layer count mismatch for {name}"
        );
        assert_eq!(
            loaded.objects.len(),
            project.objects.len(),
            "Object count mismatch for {name}"
        );
    }
}
