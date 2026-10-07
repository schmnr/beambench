//! Object selection and placement shared by the design exports.

use beambench_common::Transform2D;

use crate::object::{ObjectData, ObjectId, ProjectObject};
use crate::project::Project;

/// PostScript and PDF points per millimetre.
pub(crate) const POINTS_PER_MM: f64 = 72.0 / 25.4;

/// Objects a design export writes: visible objects on visible layers, with
/// virtual clones expanded, optionally limited to the selection. Hidden
/// layers hold helpers such as imported PDF clipping masks, which are not
/// artwork.
pub(crate) fn exportable_objects(
    project: &Project,
    selection_only: bool,
    selected_ids: &[ObjectId],
) -> Vec<ProjectObject> {
    let expanded_clones: Vec<_> = project
        .objects
        .iter()
        .filter_map(|obj| project.resolve_clone(obj))
        .collect();
    project
        .objects
        .iter()
        .filter(|obj| !matches!(obj.data, ObjectData::VirtualClone { .. }))
        .chain(expanded_clones.iter())
        .filter(|obj| !selection_only || selected_ids.contains(&obj.id))
        .filter(|obj| obj.visible)
        .filter(|obj| {
            project
                .find_layer(obj.layer_id)
                .is_none_or(|layer| layer.visible)
        })
        .cloned()
        .collect()
}

/// Map canvas millimetres (Y down from the bed's top edge) to a Y-up page in
/// `units_per_mm` whose bottom edge is the bed's bottom edge. DXF, PDF, EPS
/// and AI are all Y-up; imports use the inverse, so exports round-trip.
pub(crate) fn canvas_to_y_up(bed_height_mm: f64, units_per_mm: f64) -> Transform2D {
    Transform2D {
        a: units_per_mm,
        b: 0.0,
        c: 0.0,
        d: -units_per_mm,
        tx: 0.0,
        ty: bed_height_mm * units_per_mm,
    }
}

/// Map an image's unit square, with row zero at `y = 1` as PDF and
/// PostScript image operators draw it, onto the canvas placement of `obj`.
pub(crate) fn raster_unit_square_to_canvas(obj: &ProjectObject) -> Transform2D {
    let cx = (obj.bounds.min.x + obj.bounds.max.x) / 2.0;
    let cy = (obj.bounds.min.y + obj.bounds.max.y) / 2.0;
    let rotate_about_center = Transform2D::translate(cx, cy)
        .compose(&obj.transform)
        .compose(&Transform2D::translate(-cx, -cy));
    // Unit square with row zero at the top, Y down, onto the bounds.
    let unit_to_bounds = Transform2D {
        a: obj.bounds.width(),
        b: 0.0,
        c: 0.0,
        d: -obj.bounds.height(),
        tx: obj.bounds.min.x,
        ty: obj.bounds.max.y,
    };
    rotate_about_center.compose(&unit_to_bounds)
}

/// Six numbers for a PDF `cm` or PostScript `concat` matrix.
pub(crate) fn matrix_operands(t: &Transform2D) -> String {
    format!(
        "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6}",
        t.a, t.b, t.c, t.d, t.tx, t.ty
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{Layer, OperationType};
    use beambench_common::{Bounds, Point2D};

    fn rectangle(layer_id: crate::layer::LayerId) -> ProjectObject {
        ProjectObject::new(
            "rect",
            layer_id,
            Bounds::new(Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)),
            ObjectData::Shape {
                kind: crate::object::ShapeKind::Rectangle,
                width: 10.0,
                height: 10.0,
                corner_radius: 0.0,
            },
        )
    }

    #[test]
    fn hidden_layers_are_not_exported() {
        let mut project = Project::new("Hidden");
        let shown = project.ensure_default_layer();
        let mut hidden = Layer::new("Masks", OperationType::Tool);
        hidden.visible = false;
        let hidden = project.add_layer(hidden).id;
        project.add_object(rectangle(shown));
        project.add_object(rectangle(hidden));

        let objects = exportable_objects(&project, false, &[]);
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0].layer_id, shown);
    }

    #[test]
    fn unit_square_top_row_lands_at_the_top_of_the_bounds() {
        let mut object = rectangle(crate::layer::LayerId::new());
        object.bounds = Bounds::new(Point2D::new(10.0, 20.0), Point2D::new(30.0, 60.0));
        let t = raster_unit_square_to_canvas(&object);
        let top_left = t.apply(&Point2D::new(0.0, 1.0));
        let bottom_right = t.apply(&Point2D::new(1.0, 0.0));
        assert!((top_left.x - 10.0).abs() < 1e-9 && (top_left.y - 20.0).abs() < 1e-9);
        assert!((bottom_right.x - 30.0).abs() < 1e-9 && (bottom_right.y - 60.0).abs() < 1e-9);
    }
}
