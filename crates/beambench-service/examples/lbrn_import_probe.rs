//! Inspect an imported calibration file without opening the app or connecting hardware.
use beambench_core::{ObjectData, Project, WorkspaceOrigin};
use beambench_planner::{PlanSegment, build_plan};
use beambench_service::ServiceContext;
use beambench_service::ops::imports::{ImportFilesInput, import_files_from_paths};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("expected a .lbrn file path")?;
    let source = beambench_core::parse_lbrn_project(&std::fs::read(&path)?)?;
    for origin in [WorkspaceOrigin::TopLeft, WorkspaceOrigin::BottomLeft] {
        let ctx = ServiceContext::with_settings(Default::default());
        let mut project = Project::new("MGL import investigation");
        project.workspace.origin = origin;
        let layer_id = project.ensure_default_layer();
        *ctx.project.lock().unwrap() = Some(project);
        import_files_from_paths(
            &ctx,
            ImportFilesInput {
                create_layer: None,
                file_paths: vec![path.clone()],
                layer_id,
            },
        )?;
        let project = ctx.project.lock().unwrap().as_ref().unwrap().clone();
        let source_bitmaps: Vec<_> = source
            .shapes
            .iter()
            .filter_map(|shape| {
                let beambench_core::LbrnShape::Bitmap { data, .. } = shape else {
                    return None;
                };
                Some(data)
            })
            .collect();
        // PNG encoding changes during import. Compare decoded pixels instead.
        // This comparison is for the generator's opaque grayscale bitmaps.
        let imported_pixels: Vec<_> = project
            .asset_data
            .values()
            .map(|data| image::load_from_memory(data).map(|image| image.to_luma8()))
            .collect::<Result<_, _>>()?;
        let source_pixels: Vec<_> = source_bitmaps
            .iter()
            .map(|data| image::load_from_memory(data).map(|image| image.to_luma8()))
            .collect::<Result<_, _>>()?;
        let source_pixels_preserved = source_pixels
            .iter()
            .all(|source| imported_pixels.contains(source));
        let images: Vec<_> = project
            .objects
            .iter()
            .filter_map(|object| {
                let ObjectData::RasterImage { asset_key, .. } = &object.data else {
                    return None;
                };
                let layer = project.find_layer(object.layer_id).unwrap();
                Some(
                    json!({"bounds": object.bounds, "transform": object.transform,
                "power_scale": object.power_scale, "layer": layer.name,
                "speed": layer.primary_entry().speed_mm_min,
                "max_power": layer.primary_entry().power_percent, "asset_key": asset_key}),
                )
            })
            .collect();
        let plan = build_plan(&project)?;
        let mut output_powers = std::collections::BTreeSet::new();
        beambench_grbl::generate_gcode_to(&plan, &Default::default(), &mut |line| {
            for word in line.split_whitespace() {
                if let Some(power) = word.strip_prefix('S').and_then(|s| s.parse::<u32>().ok()) {
                    output_powers.insert(power);
                }
            }
            Ok(())
        })?;
        let raster: Vec<_> = plan
            .segments
            .iter()
            .filter_map(|segment| {
                let PlanSegment::Raster {
                    power_max_percent,
                    power_min_percent,
                    speed_mm_min,
                    scanlines,
                    ..
                } = segment
                else {
                    return None;
                };
                Some(
                    json!({"max_power": power_max_percent, "min_power": power_min_percent,
                "speed": speed_mm_min, "first_y": scanlines.first().map(|line| line.y_mm)}),
                )
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string(&json!({"origin": origin, "images": images,
            "layers": project.layers.len(), "raster_segments": raster,
            "gcode_s_values": output_powers, "source_pixels_preserved": source_pixels_preserved,
            "warnings": plan.warnings}))?
        );
    }
    Ok(())
}
