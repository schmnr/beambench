//! Shared validation helpers used by service ops to enforce
//! cross-cutting invariants on project mutations.

use beambench_common::{ColorTag, canonical_palette_color_tag};
use beambench_core::{Layer, LayerId, ObjectData, OperationType, Project};

use crate::error::{ServiceError, ServiceResult};

/// Direction of auto-routing needed for an object. `NeedsImage` means
/// a raster asset should live on an image layer; `NeedsNonImage` means
/// a vector/text/shape asset should live on a non-image layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoutingTarget {
    NeedsImage,
    NeedsNonImage,
}

impl RoutingTarget {
    /// Derive the target layer kind from an object's data variant.
    /// For `VirtualClone` the target is determined by the source's
    /// effective type (so a clone of a raster routes to an image
    /// layer).
    pub fn from_data(data: &ObjectData, project: &Project) -> Self {
        if effective_is_raster(data, project) {
            RoutingTarget::NeedsImage
        } else {
            RoutingTarget::NeedsNonImage
        }
    }

    pub(crate) fn layer_matches(&self, layer: &Layer) -> bool {
        if layer.is_tool_layer {
            return true;
        }
        match self {
            RoutingTarget::NeedsImage => layer
                .entries
                .iter()
                .any(|entry| entry.operation == OperationType::Image),
            RoutingTarget::NeedsNonImage => layer
                .entries
                .iter()
                .all(|entry| entry.operation != OperationType::Image),
        }
    }
}

/// Normalize a hex color tag for comparison: lowercase + strip 8-digit
/// RGBA to 6-digit RGB (e.g. `#FF0000FF` → `#ff0000`).
fn normalize_color_tag(hex: &str) -> String {
    let h = hex.to_lowercase();
    if h.len() == 9 && h.starts_with('#') {
        h[..7].to_string()
    } else {
        h
    }
}

/// Strip a trailing mode suffix like ` (Image)` or ` (Line)` from a
/// layer name so that re-creating a sibling doesn't stack suffixes
/// (e.g. `"C02 (Image) (Line)"` → `"C02"`).
pub fn strip_mode_suffix(name: &str) -> &str {
    const SUFFIXES: &[&str] = &[
        " (Image)",
        " (Line)",
        " (Cut)",
        " (Score)",
        " (Fill)",
        " (Offset Fill)",
    ];
    let mut s = name.trim();
    // Strip repeatedly in case multiple suffixes got stacked somehow.
    loop {
        let before = s;
        for suf in SUFFIXES {
            if let Some(stripped) = s.strip_suffix(suf) {
                s = stripped.trim();
            }
        }
        if s == before {
            break;
        }
    }
    s
}

/// Resolve a layer id that satisfies `target`, starting from
/// `requested`. Returns the same layer if it already matches;
/// otherwise prefers an existing sibling with the same `color_tag`,
/// then creates a new sibling layer next to `requested`.
///
/// Returns `Ok((resolved_layer_id, was_rerouted))`.
pub fn resolve_layer_for_object(
    project: &mut Project,
    requested: LayerId,
    target: RoutingTarget,
) -> ServiceResult<(LayerId, bool)> {
    let requested_layer = project
        .find_layer(requested)
        .ok_or_else(|| ServiceError::not_found("Layer not found"))?;
    if requested_layer.is_tool_layer {
        return Ok((requested, false));
    }
    if target.layer_matches(requested_layer) {
        return Ok((requested, false));
    }

    let requested_color_tag =
        ColorTag(canonical_palette_color_tag(&requested_layer.color_tag.0).to_string());
    let requested_name = requested_layer.name.clone();
    let requested_speed = requested_layer.primary_entry().speed_mm_min;
    let requested_power = requested_layer.primary_entry().power_percent;

    // 1. Existing matching layer with the same color_tag. Both sides go
    // through canonical_palette_color_tag so two layers whose raw colors
    // snap to the same standard palette entry count as one color family.
    let norm_tag = normalize_color_tag(&requested_color_tag.0);
    if let Some(m) = project.layers.iter().find(|l| {
        target.layer_matches(l)
            && normalize_color_tag(canonical_palette_color_tag(&l.color_tag.0)) == norm_tag
            && l.id != requested
    }) {
        return Ok((m.id, true));
    }

    // 2. Create a new sibling layer in the same family. Do NOT
    // reuse an arbitrary matching layer from some other color family:
    // Paired rows stay grouped by color tag.
    let new_operation = match target {
        RoutingTarget::NeedsImage => OperationType::Image,
        RoutingTarget::NeedsNonImage => OperationType::Line,
    };
    let suffix = match target {
        RoutingTarget::NeedsImage => " (Image)",
        RoutingTarget::NeedsNonImage => " (Line)",
    };
    let base = strip_mode_suffix(&requested_name);
    let new_name = format!("{base}{suffix}");
    let mut new_layer = Layer::new(new_name, new_operation);
    new_layer.color_tag = requested_color_tag;
    new_layer.primary_entry_mut().speed_mm_min = requested_speed;
    new_layer.primary_entry_mut().power_percent = requested_power;
    let new_layer_id = new_layer.id;
    project.layers.push(new_layer);
    Ok((new_layer_id, true))
}

/// Returns `Ok(())` if placing or keeping an object with the given
/// `ObjectData` on `dest_layer` honors the "raster and vector content
/// live on separate layers" invariant. Returns an `invalid_state`
/// error otherwise.
///
/// Rules (symmetric):
/// - `ObjectData::RasterImage` may only live on layers with
///   `OperationType::Image`.
/// - `ObjectData::VirtualClone` resolves to its source at plan time;
///   if the source is a RasterImage, the clone is effectively raster
///   content and must live on an image layer.
/// - Non-raster objects (vectors, text, shapes, etc.) may NOT live on
///   image layers.
///
/// This is called from every service op that creates an object,
/// mutates its data, or moves it to another layer, so invalid state
/// is unreachable through normal write paths. Legacy project files
/// that already carry mixed state are migrated on load via
/// `ops::persistence::migrate_mixed_layers`.
pub fn check_layer_content_invariant(
    obj_data: &ObjectData,
    dest_layer: &Layer,
    project: &Project,
) -> ServiceResult<()> {
    let is_raster = effective_is_raster(obj_data, project);
    if dest_layer.is_tool_layer {
        return Ok(());
    }
    let layer_is_image = dest_layer
        .entries
        .iter()
        .any(|entry| entry.operation == OperationType::Image);

    match (is_raster, layer_is_image) {
        (true, false) => Err(ServiceError::invalid_state(
            "Raster objects can only live on image layers",
        )),
        (false, true) => Err(ServiceError::invalid_state(
            "Image layers hold only raster objects; place vector content on a non-image layer",
        )),
        _ => Ok(()),
    }
}

/// True iff the object's effective content type (after resolving any
/// VirtualClone chain) is raster. Used by the layer-content
/// invariant so clones of raster sources are validated against the
/// raster rule, not the generic non-raster rule.
pub fn effective_is_raster(obj_data: &ObjectData, project: &Project) -> bool {
    match obj_data {
        ObjectData::RasterImage { .. } => true,
        ObjectData::VirtualClone { source_id } => project
            .find_object(*source_id)
            .map(|src| effective_is_raster(&src.data, project))
            .unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use beambench_common::{Bounds, ColorTag, Point2D};
    use beambench_core::ProjectObject;

    fn make_layer(op: OperationType) -> Layer {
        Layer::new("test", op)
    }

    fn raster_data() -> ObjectData {
        ObjectData::RasterImage {
            asset_key: "test".to_string(),
            original_width_px: 10,
            original_height_px: 10,
            adjustments: None,
            masks: Vec::new(),
        }
    }

    fn vector_data() -> ObjectData {
        ObjectData::VectorPath {
            path_data: "M 0 0 L 10 0".to_string(),
            closed: false,
            ruler_guide_axis: None,
        }
    }

    fn empty_project() -> Project {
        Project::new("test")
    }

    #[test]
    fn raster_on_image_layer_ok() {
        let layer = make_layer(OperationType::Image);
        let project = empty_project();
        assert!(check_layer_content_invariant(&raster_data(), &layer, &project).is_ok());
    }

    #[test]
    fn raster_on_line_layer_blocked() {
        let layer = make_layer(OperationType::Line);
        let project = empty_project();
        assert!(check_layer_content_invariant(&raster_data(), &layer, &project).is_err());
    }

    #[test]
    fn vector_on_line_layer_ok() {
        let layer = make_layer(OperationType::Line);
        let project = empty_project();
        assert!(check_layer_content_invariant(&vector_data(), &layer, &project).is_ok());
    }

    #[test]
    fn vector_on_image_layer_blocked() {
        let layer = make_layer(OperationType::Image);
        let project = empty_project();
        assert!(check_layer_content_invariant(&vector_data(), &layer, &project).is_err());
    }

    #[test]
    fn routing_target_matches_any_image_entry() {
        let mut layer = make_layer(OperationType::Line);
        layer
            .entries
            .push(beambench_core::CutEntry::new(OperationType::Image));
        assert!(RoutingTarget::NeedsImage.layer_matches(&layer));
        assert!(!RoutingTarget::NeedsNonImage.layer_matches(&layer));
    }

    #[test]
    fn virtual_clone_of_raster_blocked_on_line_layer() {
        // VirtualClone resolves to its source at plan time, so a
        // clone of a raster must live on an image layer.
        let mut project = empty_project();
        let image_layer = make_layer(OperationType::Image);
        let image_layer_id = image_layer.id;
        project.layers.push(image_layer);
        let raster_obj = ProjectObject::new(
            "src",
            image_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            raster_data(),
        );
        let raster_id = raster_obj.id;
        project.add_object(raster_obj);

        let clone = ObjectData::VirtualClone {
            source_id: raster_id,
        };
        let line_layer = make_layer(OperationType::Line);
        assert!(
            check_layer_content_invariant(&clone, &line_layer, &project).is_err(),
            "VirtualClone of raster must be blocked on a non-image layer",
        );

        let image_layer_dest = make_layer(OperationType::Image);
        assert!(check_layer_content_invariant(&clone, &image_layer_dest, &project).is_ok());
    }

    #[test]
    fn virtual_clone_of_vector_blocked_on_image_layer() {
        let mut project = empty_project();
        let line_layer = make_layer(OperationType::Line);
        let line_layer_id = line_layer.id;
        project.layers.push(line_layer);
        let vec_obj = ProjectObject::new(
            "src",
            line_layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            vector_data(),
        );
        let vec_id = vec_obj.id;
        project.add_object(vec_obj);

        let clone = ObjectData::VirtualClone { source_id: vec_id };
        let image_layer = make_layer(OperationType::Image);
        assert!(
            check_layer_content_invariant(&clone, &image_layer, &project).is_err(),
            "VirtualClone of vector must be blocked on an image layer",
        );

        let line_layer_dest = make_layer(OperationType::Line);
        assert!(check_layer_content_invariant(&clone, &line_layer_dest, &project).is_ok());
    }

    #[test]
    fn resolve_layer_for_object_prefers_same_color_sibling() {
        let mut project = empty_project();

        let mut requested = make_layer(OperationType::Image);
        requested.color_tag = ColorTag("#ff0000".to_string());
        requested.name = "Red".to_string();
        let requested_id = requested.id;
        project.layers.push(requested);

        let mut same_family_line = make_layer(OperationType::Line);
        same_family_line.color_tag = ColorTag("#ff0000".to_string());
        let same_family_line_id = same_family_line.id;
        project.layers.push(same_family_line);

        let mut other_family_line = make_layer(OperationType::Line);
        other_family_line.color_tag = ColorTag("#00ff00".to_string());
        project.layers.push(other_family_line);

        let (resolved, rerouted) =
            resolve_layer_for_object(&mut project, requested_id, RoutingTarget::NeedsNonImage)
                .expect("resolver should succeed");
        assert!(rerouted);
        assert_eq!(resolved, same_family_line_id);
    }

    #[test]
    fn resolve_layer_for_object_does_not_reuse_other_color_family() {
        let mut project = empty_project();

        let mut requested = make_layer(OperationType::Image);
        requested.color_tag = ColorTag("#ff0000".to_string());
        requested.name = "Red".to_string();
        let requested_id = requested.id;
        project.layers.push(requested);

        let mut other_family_line = make_layer(OperationType::Line);
        other_family_line.color_tag = ColorTag("#00ff00".to_string());
        let other_family_line_id = other_family_line.id;
        project.layers.push(other_family_line);

        let before_len = project.layers.len();
        let (resolved, rerouted) =
            resolve_layer_for_object(&mut project, requested_id, RoutingTarget::NeedsNonImage)
                .expect("resolver should succeed");
        assert!(rerouted);
        assert_ne!(resolved, other_family_line_id);
        assert_eq!(project.layers.len(), before_len + 1);
        let created = project
            .find_layer(resolved)
            .expect("new sibling should exist");
        assert_eq!(created.color_tag, ColorTag("#FF0000".to_string()));
        assert_eq!(created.primary_entry().operation, OperationType::Line);
    }

    #[test]
    fn resolve_matches_sibling_with_alpha_suffix_color_tag() {
        let mut project = empty_project();

        // Requested layer has 8-digit RGBA color tag.
        let mut requested = make_layer(OperationType::Image);
        requested.color_tag = ColorTag("#ff0000ff".to_string());
        requested.name = "Red".to_string();
        let requested_id = requested.id;
        project.layers.push(requested);

        // Existing sibling has 6-digit RGB tag — same visual color.
        let mut sibling = make_layer(OperationType::Line);
        sibling.color_tag = ColorTag("#ff0000".to_string());
        let sibling_id = sibling.id;
        project.layers.push(sibling);

        let (resolved, rerouted) =
            resolve_layer_for_object(&mut project, requested_id, RoutingTarget::NeedsNonImage)
                .expect("resolver should succeed");
        assert!(rerouted);
        assert_eq!(
            resolved, sibling_id,
            "should match sibling despite alpha suffix difference"
        );
    }

    #[test]
    fn resolve_strips_stale_mode_suffix_from_sibling_name() {
        let mut project = empty_project();

        // Layer already has a mode suffix in its name.
        let mut requested = make_layer(OperationType::Image);
        requested.color_tag = ColorTag("#ff0000".to_string());
        requested.name = "C01 (Image)".to_string();
        let requested_id = requested.id;
        project.layers.push(requested);

        let (resolved, _) =
            resolve_layer_for_object(&mut project, requested_id, RoutingTarget::NeedsNonImage)
                .expect("resolver should succeed");
        let created = project
            .find_layer(resolved)
            .expect("new sibling should exist");
        assert_eq!(
            created.name, "C01 (Line)",
            "should strip existing (Image) suffix before appending (Line)"
        );
    }

    #[test]
    fn strip_mode_suffix_handles_stacked_suffixes() {
        assert_eq!(strip_mode_suffix("C02 (Image) (Line)"), "C02");
        assert_eq!(strip_mode_suffix("C02 (Image)"), "C02");
        assert_eq!(strip_mode_suffix("My Layer"), "My Layer");
        assert_eq!(strip_mode_suffix("  C01 (Fill)  "), "C01");
    }

    #[test]
    fn normalize_color_tag_strips_alpha() {
        assert_eq!(normalize_color_tag("#FF0000FF"), "#ff0000");
        assert_eq!(normalize_color_tag("#ff0000"), "#ff0000");
        assert_eq!(normalize_color_tag("#FF0000"), "#ff0000");
        assert_eq!(normalize_color_tag("#00ff00ff"), "#00ff00");
    }
}
