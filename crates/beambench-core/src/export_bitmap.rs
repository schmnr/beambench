//! Export bitmap files.

use crate::AssetId;
use crate::layer::Layer;
use crate::object::{ImageMaskPolarity, ObjectData, ObjectId, ProjectObject};
use crate::project::Project;
use crate::vector::flatten::{DEFAULT_TOLERANCE_MM, flatten_vecpath};
use crate::vector::offset::signed_area;
use beambench_common::geometry::Point2D;
use beambench_common::{Polyline, RasterMode};
use beambench_raster::{ProcessedRaster, RasterPixelFormat, RasterProcessingParams};
use image::{GrayImage, ImageEncoder, Luma};
use std::io::Write;
use std::path::Path;

/// Save the processed bitmap for a raster object to a PNG file.
///
/// Loads the selected raster asset, applies the same image/raster processing
/// parameters used by the planner, applies image masks, and writes the result
/// as an 8-bit grayscale PNG. Binary/dithered modes are expanded to black and
/// white pixels so the saved image matches the laser-ready bitmap.
pub fn save_processed_bitmap(
    project: &Project,
    object_id: ObjectId,
    output_path: &Path,
) -> Result<(), String> {
    let img = processed_bitmap_image(project, object_id)?;
    let file =
        std::fs::File::create(output_path).map_err(|e| format!("Failed to create file: {e}"))?;
    write_gray_png(&img, file)
}

/// Render the processed bitmap for a raster object as PNG bytes.
pub fn processed_bitmap_png(project: &Project, object_id: ObjectId) -> Result<Vec<u8>, String> {
    let img = processed_bitmap_image(project, object_id)?;
    let mut bytes = Vec::new();
    write_gray_png(&img, &mut bytes)?;
    Ok(bytes)
}

fn processed_bitmap_image(project: &Project, object_id: ObjectId) -> Result<GrayImage, String> {
    let selected_obj = project
        .find_object(object_id)
        .ok_or_else(|| format!("Object not found: {object_id}"))?;
    processed_bitmap_image_for_object(project, selected_obj)
}

/// Render the processed bitmap for an already-resolved raster object as PNG bytes.
pub fn processed_bitmap_png_for_object(
    project: &Project,
    obj: &crate::ProjectObject,
) -> Result<Vec<u8>, String> {
    let img = processed_bitmap_image_for_object(project, obj)?;
    let mut bytes = Vec::new();
    write_gray_png(&img, &mut bytes)?;
    Ok(bytes)
}

fn processed_bitmap_image_for_object(
    project: &Project,
    obj: &crate::ProjectObject,
) -> Result<GrayImage, String> {
    let resolved_obj = project.resolve_clone(obj);
    let obj = resolved_obj.as_ref().unwrap_or(obj);

    let (asset_key, adjustments) = match &obj.data {
        ObjectData::RasterImage {
            asset_key,
            adjustments,
            ..
        } => (asset_key.clone(), adjustments.clone().unwrap_or_default()),
        _ => return Err(format!("Object '{}' is not a raster image", obj.id)),
    };

    let layer = project
        .find_layer(obj.layer_id)
        .ok_or_else(|| format!("Layer not found for object: {}", obj.id))?;

    let source_bytes = find_asset_data_by_key(&project.asset_data, &asset_key)
        .ok_or_else(|| format!("Asset data not found for key: {asset_key}"))?;

    let params = build_processed_bitmap_params(
        layer,
        adjustments,
        source_bytes,
        (obj.bounds.width(), obj.bounds.height()),
    );
    let processed = beambench_raster::process_raster(params)
        .map_err(|e| format!("Failed to process raster: {e}"))?;
    let processed = apply_image_masks(project, obj, &processed);
    processed_raster_to_gray_image(&processed)
}

fn build_processed_bitmap_params(
    layer: &Layer,
    adjustments: beambench_common::RasterAdjustments,
    source_bytes: Vec<u8>,
    bounds_mm: (f64, f64),
) -> RasterProcessingParams {
    let rs = layer.primary_entry().raster_settings.as_ref();
    RasterProcessingParams {
        source_bytes,
        bounds_mm,
        dpi: rs.map(|s| s.effective_dpi()).unwrap_or(254),
        mode: rs.map(|s| s.mode).unwrap_or(RasterMode::FloydSteinberg),
        adjustments,
        pass_through: rs.map(|s| s.pass_through).unwrap_or(false),
        halftone_cells_per_inch: rs.map(|s| s.halftone_cells_per_inch).unwrap_or(10),
        halftone_angle_deg: rs.map(|s| s.halftone_angle_deg).unwrap_or(0.0),
        newsprint_angle_deg: rs.map(|s| s.newsprint_angle_deg).unwrap_or(45.0),
        newsprint_frequency: rs.map(|s| s.newsprint_frequency).unwrap_or(10.0),
        invert: rs.map(|s| s.invert).unwrap_or(false),
    }
}

fn apply_image_masks(
    project: &Project,
    obj: &ProjectObject,
    processed: &ProcessedRaster,
) -> ProcessedRaster {
    let ObjectData::RasterImage { masks, .. } = &obj.data else {
        return processed.clone();
    };
    if masks.is_empty() {
        return processed.clone();
    }

    let mut inside = Vec::new();
    let mut outside = Vec::new();
    for mask in masks {
        let Some(mask_obj) = project.find_object(mask.object_id) else {
            continue;
        };
        let Some(path) =
            crate::vector::convert::object_to_world_vecpath_resolved(mask_obj, project)
        else {
            continue;
        };
        let polylines: Vec<Polyline> = flatten_vecpath(&path, DEFAULT_TOLERANCE_MM)
            .into_iter()
            .filter(polyline_has_area)
            .collect();
        if polylines.is_empty() {
            continue;
        }
        match mask.polarity {
            ImageMaskPolarity::KeepInside => inside.extend(polylines),
            ImageMaskPolarity::KeepOutside => outside.extend(polylines),
        }
    }

    if inside.is_empty() && outside.is_empty() {
        return processed.clone();
    }

    let mut masked = processed.clone();
    let x_step = processed.effective_x_pixel_mm();
    let y_step = processed.line_interval_mm;
    let obj_center = Point2D::new(
        (obj.bounds.min.x + obj.bounds.max.x) / 2.0,
        (obj.bounds.min.y + obj.bounds.max.y) / 2.0,
    );
    for y in 0..processed.height_px as usize {
        for x in 0..processed.width_px as usize {
            let local = Point2D::new(
                obj.bounds.min.x + (x as f64 + 0.5) * x_step,
                obj.bounds.min.y + (y as f64 + 0.5) * y_step,
            );
            let world = if obj.transform.is_identity() {
                local
            } else {
                obj.transform.apply_around_center(&local, &obj_center)
            };
            let inside_ok = inside.is_empty() || point_in_any_polyline(world, &inside);
            let outside_hit = !outside.is_empty() && point_in_any_polyline(world, &outside);
            if !inside_ok || outside_hit {
                set_raster_pixel_unburned(&mut masked, x, y);
            }
        }
    }

    masked
}

fn set_raster_pixel_unburned(raster: &mut ProcessedRaster, x: usize, y: usize) {
    match raster.format {
        RasterPixelFormat::Binary => {
            let row_bytes = (raster.width_px as usize).div_ceil(8);
            let idx = y * row_bytes + x / 8;
            if let Some(byte) = raster.data.get_mut(idx) {
                let bit_idx = 7 - (x % 8);
                *byte |= 1 << bit_idx;
            }
        }
        RasterPixelFormat::Grayscale8 => {
            let idx = y * raster.width_px as usize + x;
            if let Some(pixel) = raster.data.get_mut(idx) {
                *pixel = 255;
            }
        }
    }
}

fn polyline_has_area(poly: &Polyline) -> bool {
    poly.closed && poly.points.len() >= 3 && signed_area(&poly.points).abs() > 1e-9
}

fn point_in_polyline(point: Point2D, poly: &Polyline) -> bool {
    let pts = &poly.points;
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i].x, pts[i].y);
        let (xj, yj) = (pts[j].x, pts[j].y);
        if ((yi > point.y) != (yj > point.y))
            && (point.x < (xj - xi) * (point.y - yi) / (yj - yi) + xi)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn point_in_any_polyline(point: Point2D, polys: &[Polyline]) -> bool {
    let mut winding: i32 = 0;
    for poly in polys {
        if !polyline_has_area(poly) {
            continue;
        }
        if point_in_polyline(point, poly) {
            winding += if signed_area(&poly.points) >= 0.0 {
                1
            } else {
                -1
            };
        }
    }
    winding != 0
}

fn processed_raster_to_gray_image(processed: &ProcessedRaster) -> Result<GrayImage, String> {
    match processed.format {
        RasterPixelFormat::Grayscale8 => GrayImage::from_raw(
            processed.width_px,
            processed.height_px,
            processed.data.clone(),
        )
        .ok_or_else(|| "Processed grayscale raster has invalid dimensions".to_string()),
        RasterPixelFormat::Binary => {
            let row_bytes = (processed.width_px as usize).div_ceil(8);
            let mut img = GrayImage::new(processed.width_px, processed.height_px);
            for y in 0..processed.height_px as usize {
                for x in 0..processed.width_px as usize {
                    let idx = y * row_bytes + x / 8;
                    let bit_idx = 7 - (x % 8);
                    let value = if processed
                        .data
                        .get(idx)
                        .is_some_and(|byte| (byte & (1 << bit_idx)) != 0)
                    {
                        255
                    } else {
                        0
                    };
                    img.put_pixel(x as u32, y as u32, Luma([value]));
                }
            }
            Ok(img)
        }
    }
}

fn write_gray_png<W: Write>(img: &GrayImage, writer: W) -> Result<(), String> {
    let encoder = image::codecs::png::PngEncoder::new(writer);
    encoder
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::L8,
        )
        .map_err(|e| format!("Failed to write PNG: {e}"))?;

    Ok(())
}

fn find_asset_data_by_key(
    asset_data: &std::collections::HashMap<AssetId, Vec<u8>>,
    asset_key: &str,
) -> Option<Vec<u8>> {
    for (id, data) in asset_data {
        if id.to_string() == asset_key {
            return Some(data.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::{Asset, AssetMediaType};
    use crate::layer::{Layer, OperationType};
    use crate::object::{ImageMaskRef, ProjectObject};
    use beambench_common::RasterAdjustments;
    use beambench_common::geometry::{Bounds, Point2D};

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

    fn add_png_asset(project: &mut Project, img: &GrayImage) -> String {
        let png_bytes = encode_png(img);
        let asset = Asset::new(
            "test.png",
            AssetMediaType::Png,
            png_bytes.len() as u64,
            Some(img.width()),
            Some(img.height()),
        );
        let asset_key = asset.id.to_string();
        project.add_asset(asset, png_bytes);
        asset_key
    }

    fn add_image_layer(
        project: &mut Project,
        mode: RasterMode,
        line_interval_mm: f64,
        invert: bool,
    ) -> crate::layer::LayerId {
        let mut layer = Layer::new("Image", OperationType::Image);
        let rs = layer.primary_entry_mut().raster_settings.as_mut().unwrap();
        rs.mode = mode;
        rs.line_interval_mm = line_interval_mm;
        rs.invert = invert;
        let layer_id = layer.id;
        project.add_layer(layer);
        layer_id
    }

    fn save_and_decode(project: &Project, object_id: ObjectId) -> GrayImage {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.png");
        save_processed_bitmap(project, object_id, &output).unwrap();
        let saved_bytes = std::fs::read(&output).unwrap();
        image::load_from_memory(&saved_bytes).unwrap().to_luma8()
    }

    #[test]
    fn save_processed_bitmap_threshold_outputs_black_and_white() {
        let mut project = Project::new("test");
        let source_img = GrayImage::from_vec(2, 1, vec![64, 192]).unwrap();
        let asset_key = add_png_asset(&mut project, &source_img);
        let layer_id = add_image_layer(&mut project, RasterMode::Threshold, 1.0, false);
        let obj = ProjectObject::new(
            "threshold",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 1.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 2,
                original_height_px: 1,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let saved_img = save_and_decode(&project, oid);
        assert_eq!(saved_img.dimensions(), (2, 1));
        assert_eq!(saved_img.get_pixel(0, 0)[0], 0);
        assert_eq!(saved_img.get_pixel(1, 0)[0], 255);
    }

    #[test]
    fn save_processed_bitmap_grayscale_preserves_adjusted_tones() {
        let mut project = Project::new("test");
        let source_img = GrayImage::from_vec(2, 1, vec![100, 200]).unwrap();
        let asset_key = add_png_asset(&mut project, &source_img);
        let layer_id = add_image_layer(&mut project, RasterMode::Grayscale, 1.0, false);
        let obj = ProjectObject::new(
            "grayscale",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 1.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 2,
                original_height_px: 1,
                adjustments: Some(RasterAdjustments {
                    brightness: 0.1,
                    ..RasterAdjustments::default()
                }),
                masks: Vec::new(),
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let saved_img = save_and_decode(&project, oid);
        assert_eq!(saved_img.dimensions(), (2, 1));
        assert!(saved_img.get_pixel(0, 0)[0] > 120);
        assert!(saved_img.get_pixel(0, 0)[0] < 130);
        assert!(saved_img.get_pixel(1, 0)[0] > 220);
        assert!(saved_img.get_pixel(1, 0)[0] < 230);
    }

    #[test]
    fn save_processed_bitmap_uses_layer_invert_and_effective_dpi() {
        let mut project = Project::new("test");
        let source_img = GrayImage::from_pixel(1, 1, Luma([0]));
        let asset_key = add_png_asset(&mut project, &source_img);
        let layer_id = add_image_layer(&mut project, RasterMode::Grayscale, 0.5, true);
        let obj = ProjectObject::new(
            "inverted",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 1,
                original_height_px: 1,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let saved_img = save_and_decode(&project, oid);
        assert_eq!(saved_img.dimensions(), (2, 2));
        assert!(saved_img.pixels().all(|pixel| pixel[0] == 255));
    }

    #[test]
    fn save_processed_bitmap_applies_image_masks() {
        let mut project = Project::new("test");
        let source_img = GrayImage::from_vec(2, 1, vec![0, 0]).unwrap();
        let asset_key = add_png_asset(&mut project, &source_img);
        let layer_id = add_image_layer(&mut project, RasterMode::Threshold, 1.0, false);

        let mask = ProjectObject::new(
            "mask",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(1.0, 1.0)),
            ObjectData::VectorPath {
                path_data: "M0,0 L1,0 L1,1 L0,1 Z".to_string(),
                closed: true,
                ruler_guide_axis: None,
            },
        );
        let mask_id = mask.id;
        project.add_object(mask);

        let obj = ProjectObject::new(
            "masked",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(2.0, 1.0)),
            ObjectData::RasterImage {
                asset_key,
                original_width_px: 2,
                original_height_px: 1,
                adjustments: None,
                masks: vec![ImageMaskRef {
                    object_id: mask_id,
                    polarity: ImageMaskPolarity::KeepInside,
                }],
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let saved_img = save_and_decode(&project, oid);
        assert_eq!(saved_img.dimensions(), (2, 1));
        assert_eq!(saved_img.get_pixel(0, 0)[0], 0);
        assert_eq!(saved_img.get_pixel(1, 0)[0], 255);
    }

    #[test]
    fn save_processed_bitmap_missing_asset_returns_error() {
        let mut project = Project::new("test");
        let layer_id = add_image_layer(&mut project, RasterMode::Threshold, 1.0, false);

        let obj = ProjectObject::new(
            "raster_missing",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::RasterImage {
                asset_key: "nonexistent_asset_id".to_string(),
                original_width_px: 4,
                original_height_px: 4,
                adjustments: None,
                masks: Vec::new(),
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.png");
        let result = save_processed_bitmap(&project, oid, &output);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Asset data not found"));
    }

    #[test]
    fn save_processed_bitmap_non_raster_returns_error() {
        let mut project = Project::new("test");
        let layer_id = add_image_layer(&mut project, RasterMode::Threshold, 1.0, false);
        let obj = ProjectObject::new(
            "vector",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::VectorPath {
                path_data: "M0,0 L10,0".to_string(),
                closed: false,
                ruler_guide_axis: None,
            },
        );
        let oid = obj.id;
        project.add_object(obj);

        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.png");
        let result = save_processed_bitmap(&project, oid, &output);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("is not a raster image"));
    }
}
