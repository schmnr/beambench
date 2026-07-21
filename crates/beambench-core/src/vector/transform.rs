use beambench_common::geometry::Transform2D;
use beambench_common::path::{PathCommand, SubPath, VecPath};

/// Apply an affine transform to every control point in a VecPath,
/// returning a new path with identity transform baked in.
pub fn bake_transform(path: &VecPath, transform: &Transform2D) -> VecPath {
    if transform.is_identity() {
        return path.clone();
    }

    let subpaths = path
        .subpaths
        .iter()
        .map(|sp| {
            let commands = sp
                .commands
                .iter()
                .map(|cmd| transform_command(cmd, transform))
                .collect();
            SubPath {
                commands,
                closed: sp.closed,
            }
        })
        .collect();

    VecPath { subpaths }
}

fn transform_command(cmd: &PathCommand, t: &Transform2D) -> PathCommand {
    use beambench_common::geometry::Point2D;

    match *cmd {
        PathCommand::MoveTo { x, y } => {
            let p = t.apply(&Point2D::new(x, y));
            PathCommand::MoveTo { x: p.x, y: p.y }
        }
        PathCommand::LineTo { x, y } => {
            let p = t.apply(&Point2D::new(x, y));
            PathCommand::LineTo { x: p.x, y: p.y }
        }
        PathCommand::QuadTo { cx, cy, x, y } => {
            let cp = t.apply(&Point2D::new(cx, cy));
            let ep = t.apply(&Point2D::new(x, y));
            PathCommand::QuadTo {
                cx: cp.x,
                cy: cp.y,
                x: ep.x,
                y: ep.y,
            }
        }
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => {
            let cp1 = t.apply(&Point2D::new(c1x, c1y));
            let cp2 = t.apply(&Point2D::new(c2x, c2y));
            let ep = t.apply(&Point2D::new(x, y));
            PathCommand::CubicTo {
                c1x: cp1.x,
                c1y: cp1.y,
                c2x: cp2.x,
                c2y: cp2.y,
                x: ep.x,
                y: ep.y,
            }
        }
        PathCommand::Close => PathCommand::Close,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_transform_preserves_path() {
        let path = VecPath::parse_svg_d("M10 20 L30 40 Z");
        let result = bake_transform(&path, &Transform2D::identity());
        assert_eq!(path, result);
    }

    #[test]
    fn translate_moves_all_points() {
        let path = VecPath::parse_svg_d("M0 0 L10 10");
        let t = Transform2D::translate(5.0, 3.0);
        let result = bake_transform(&path, &t);

        let cmds = &result.subpaths[0].commands;
        assert_eq!(cmds[0], PathCommand::MoveTo { x: 5.0, y: 3.0 });
        assert_eq!(cmds[1], PathCommand::LineTo { x: 15.0, y: 13.0 });
    }

    #[test]
    fn scale_scales_all_points() {
        let path = VecPath::parse_svg_d("M10 20 L30 40");
        let t = Transform2D::scale(2.0, 0.5);
        let result = bake_transform(&path, &t);

        let cmds = &result.subpaths[0].commands;
        assert_eq!(cmds[0], PathCommand::MoveTo { x: 20.0, y: 10.0 });
        assert_eq!(cmds[1], PathCommand::LineTo { x: 60.0, y: 20.0 });
    }

    #[test]
    fn rotation_transforms_correctly() {
        let path = VecPath::parse_svg_d("M1 0");
        let t = Transform2D::rotate(std::f64::consts::FRAC_PI_2);
        let result = bake_transform(&path, &t);

        if let PathCommand::MoveTo { x, y } = result.subpaths[0].commands[0] {
            assert!(x.abs() < 1e-10, "Expected ~0, got {x}");
            assert!((y - 1.0).abs() < 1e-10, "Expected ~1, got {y}");
        } else {
            panic!("Expected MoveTo");
        }
    }

    #[test]
    fn bake_preserves_close_and_closed_flag() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 Z");
        let t = Transform2D::translate(1.0, 1.0);
        let result = bake_transform(&path, &t);
        assert!(result.subpaths[0].closed);
        assert_eq!(
            result.subpaths[0].commands.last().unwrap(),
            &PathCommand::Close
        );
    }

    #[test]
    fn bake_cubic_transforms_all_control_points() {
        let path = VecPath::parse_svg_d("M0 0 C10 20 30 40 50 60");
        let t = Transform2D::translate(1.0, 2.0);
        let result = bake_transform(&path, &t);

        if let PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } = result.subpaths[0].commands[1]
        {
            assert_eq!(c1x, 11.0);
            assert_eq!(c1y, 22.0);
            assert_eq!(c2x, 31.0);
            assert_eq!(c2y, 42.0);
            assert_eq!(x, 51.0);
            assert_eq!(y, 62.0);
        } else {
            panic!("Expected CubicTo");
        }
    }
}
