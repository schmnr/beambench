//! Shared builder for `RasterProcessingParams` used for image-layer rasters.
//!
//! Used by the planner when building `PlanSegment::Raster` from an image
//! object. Kept as a dedicated helper so the exact parameter-assembly rules
//! are easy to audit and share if other consumers appear.

use beambench_common::{RasterAdjustments, RasterMode};
use beambench_core::Layer;
use beambench_raster::RasterProcessingParams;

/// Build `RasterProcessingParams` for a single raster object on the given
/// layer.
///
/// `adjustments` is the per-object adjustments payload (falling back to
/// `Default` if the object has none).
/// `source_bytes` is the image asset bytes.
/// `bounds_mm` is the object's target size in millimeters (width, height).
pub fn build_image_raster_params(
    layer: &Layer,
    adjustments: RasterAdjustments,
    source_bytes: Vec<u8>,
    bounds_mm: (f64, f64),
) -> RasterProcessingParams {
    let rs = layer.primary_entry().raster_settings.as_ref();
    RasterProcessingParams {
        source_bytes,
        bounds_mm,
        // Use the canonical effective_dpi() so `line_interval_mm` edits
        // take effect even when the legacy `dpi` field is stale.
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

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::RasterMode;
    use beambench_core::{Layer, OperationType};

    #[test]
    fn builds_params_from_layer_raster_settings() {
        let mut layer = Layer::new("Image", OperationType::Image);
        let rs = layer.primary_entry_mut().raster_settings.as_mut().unwrap();
        // Set both consistently so the effective-dpi helper picks up 508.
        rs.line_interval_mm = 25.4 / 508.0;
        rs.dpi = 508;
        rs.mode = RasterMode::Jarvis;
        rs.halftone_cells_per_inch = 20;
        rs.halftone_angle_deg = 15.0;
        rs.newsprint_angle_deg = 30.0;
        rs.newsprint_frequency = 8.0;
        rs.pass_through = true;

        let adj = RasterAdjustments {
            brightness: 10.0,
            contrast: 20.0,
            gamma: 1.2,
            ..RasterAdjustments::default()
        };
        let params = build_image_raster_params(&layer, adj.clone(), vec![1, 2, 3], (100.0, 50.0));
        assert_eq!(params.dpi, 508);
        assert_eq!(params.mode, RasterMode::Jarvis);
        assert_eq!(params.halftone_cells_per_inch, 20);
        assert_eq!(params.halftone_angle_deg, 15.0);
        assert_eq!(params.newsprint_angle_deg, 30.0);
        assert_eq!(params.newsprint_frequency, 8.0);
        assert!(params.pass_through);
        assert_eq!(params.bounds_mm, (100.0, 50.0));
        assert_eq!(params.source_bytes, vec![1, 2, 3]);
        assert_eq!(params.adjustments.brightness, 10.0);
    }

    #[test]
    fn threads_layer_invert_through_params() {
        let mut layer = Layer::new("Image", OperationType::Image);
        let rs = layer.primary_entry_mut().raster_settings.as_mut().unwrap();
        rs.invert = true;

        let params =
            build_image_raster_params(&layer, RasterAdjustments::default(), vec![], (10.0, 10.0));
        assert!(params.invert);
    }

    #[test]
    fn prefers_effective_dpi_from_line_interval() {
        // Setting a non-default line_interval_mm should win over the
        // legacy dpi field.
        let mut layer = Layer::new("Image", OperationType::Image);
        let rs = layer.primary_entry_mut().raster_settings.as_mut().unwrap();
        rs.dpi = 254;
        rs.line_interval_mm = 0.05; // => 508 dpi

        let params =
            build_image_raster_params(&layer, RasterAdjustments::default(), vec![], (10.0, 10.0));
        assert_eq!(params.dpi, 508);
    }

    #[test]
    fn uses_defaults_when_raster_settings_missing() {
        // Force a layer with no raster_settings — same defaults as the
        // planner's legacy inline builder.
        let mut layer = Layer::new("Cut", OperationType::Cut);
        layer.primary_entry_mut().raster_settings = None;

        let params =
            build_image_raster_params(&layer, RasterAdjustments::default(), vec![], (10.0, 10.0));
        assert_eq!(params.dpi, 254);
        assert_eq!(params.mode, RasterMode::FloydSteinberg);
        assert_eq!(params.halftone_cells_per_inch, 10);
        assert_eq!(params.halftone_angle_deg, 0.0);
        assert_eq!(params.newsprint_angle_deg, 45.0);
        assert_eq!(params.newsprint_frequency, 10.0);
        assert!(!params.pass_through);
    }
}
