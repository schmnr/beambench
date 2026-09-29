//! Synthetic worst-case raster benchmark. Run the built binary under `/usr/bin/time -l`.
use beambench_common::{Bounds, Point2D};
use beambench_core::{
    Asset, AssetMediaType, Layer, ObjectData, OperationType, Project, ProjectObject,
};
use beambench_grbl::{GcodeConfig, GcodeSpool};
use beambench_planner::build_plan;
use sha2::{Digest, Sha256};

fn main() {
    let height: u32 = std::env::args()
        .nth(1)
        .unwrap_or("1000".into())
        .parse()
        .unwrap();
    let width = 2000;
    let grayscale = std::env::args().nth(2).as_deref() == Some("gray");
    let img = image::GrayImage::from_fn(width, height, |x, y| {
        image::Luma([if (x + y) % 2 == 0 {
            if grayscale { (x % 255) as u8 } else { 0 }
        } else {
            255
        }])
    });
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        img.as_raw(),
        width,
        height,
        image::ExtendedColorType::L8,
    )
    .unwrap();
    drop(img);
    let mut project = Project::new("raster-memory");
    project.workspace.bed_width_mm = 1000.0;
    project.workspace.bed_height_mm = 1000.0;
    let mut layer = Layer::new("Image", OperationType::Image);
    layer
        .primary_entry_mut()
        .raster_settings
        .as_mut()
        .unwrap()
        .pass_through = !grayscale;
    if grayscale {
        layer
            .primary_entry_mut()
            .raster_settings
            .as_mut()
            .unwrap()
            .mode = beambench_common::RasterMode::Grayscale;
    }
    let asset = Asset::new(
        "checker.png",
        AssetMediaType::Png,
        png.len() as u64,
        Some(width),
        Some(height),
    );
    let key = asset.id.to_string();
    project.add_asset(asset, png);
    project.add_object(ProjectObject::new(
        "checker",
        layer.id,
        Bounds::new(
            Point2D::new(20.0, 20.0),
            Point2D::new(220.0, 20.0 + height as f64 * 0.1),
        ),
        ObjectData::RasterImage {
            asset_key: key,
            original_width_px: width,
            original_height_px: height,
            adjustments: None,
            masks: vec![],
        },
    ));
    project.layers.push(layer);
    let plan = build_plan(&project).unwrap();
    let burn_runs: usize = plan
        .segments
        .iter()
        .filter_map(|segment| match segment {
            beambench_planner::PlanSegment::Raster { scanlines, .. } => {
                Some(scanlines.iter().map(|row| row.runs.len()).sum::<usize>())
            }
            _ => None,
        })
        .sum();
    let cached = plan.clone();
    let display = plan.clone();
    let preview = beambench_preview::distill_preview(&display);
    match std::env::args().nth(2).as_deref() {
        Some("ruida") => {
            let job = beambench_ruida::compile_ruida_job(&plan, &Default::default()).unwrap();
            println!(
                "ruida clear_bytes={} rd_bytes={} sha256={:x}",
                job.clear_bytes.len(),
                job.rd_file_bytes.len(),
                Sha256::digest(&job.clear_bytes)
            );
            return;
        }
        Some("lihuiyu") => {
            match beambench_lihuiyu::compile_lihuiyu_job(&plan, &Default::default()) {
                Ok(job) => println!("lihuiyu: {:#?}", job.summary),
                Err(error) => println!("lihuiyu rejected: {error}"),
            }
            return;
        }
        _ => {}
    }
    let mut commands = GcodeSpool::generate(&plan, &GcodeConfig::default()).unwrap();
    let mut hash = Sha256::new();
    while let Some(line) = commands.next_line().unwrap() {
        hash.update(line.as_bytes());
        hash.update(b"\n");
    }
    println!(
        "runs={} lines={} sha256={:x} preview_layers={} segments={}",
        burn_runs,
        commands.len(),
        hash.finalize(),
        preview.layers.len(),
        cached.segments.len()
    );
}
