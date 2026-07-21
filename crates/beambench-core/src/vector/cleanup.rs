use beambench_common::path::{PathCommand, VecPath};

/// Remove zero-length segments (consecutive identical endpoints).
pub fn remove_zero_length_segments(path: &mut VecPath) {
    for subpath in &mut path.subpaths {
        let mut prev_point: Option<(f64, f64)> = None;
        subpath.commands.retain(|cmd| {
            let point = match *cmd {
                PathCommand::MoveTo { x, y } => {
                    prev_point = Some((x, y));
                    return true;
                }
                PathCommand::LineTo { x, y } => Some((x, y)),
                PathCommand::QuadTo { x, y, .. } => Some((x, y)),
                PathCommand::CubicTo { x, y, .. } => Some((x, y)),
                PathCommand::Close => return true,
            };
            if let (Some(prev), Some(cur)) = (prev_point, point) {
                let dx = cur.0 - prev.0;
                let dy = cur.1 - prev.1;
                if dx * dx + dy * dy < 1e-20 {
                    return false; // Zero-length segment
                }
                prev_point = point;
            } else {
                prev_point = point;
            }
            true
        });
    }
}

/// Deduplicate consecutive points that are within tolerance of each other.
pub fn dedup_consecutive_points(path: &mut VecPath, tolerance: f64) {
    let tol_sq = tolerance * tolerance;
    for subpath in &mut path.subpaths {
        let mut prev_point: Option<(f64, f64)> = None;
        subpath.commands.retain(|cmd| {
            let point = match *cmd {
                PathCommand::MoveTo { x, y } => {
                    prev_point = Some((x, y));
                    return true;
                }
                PathCommand::LineTo { x, y } => Some((x, y)),
                PathCommand::QuadTo { x, y, .. } => Some((x, y)),
                PathCommand::CubicTo { x, y, .. } => Some((x, y)),
                PathCommand::Close => return true,
            };
            if let (Some(prev), Some(cur)) = (prev_point, point) {
                let dx = cur.0 - prev.0;
                let dy = cur.1 - prev.1;
                if dx * dx + dy * dy < tol_sq {
                    return false;
                }
                prev_point = point;
            } else {
                prev_point = point;
            }
            true
        });
    }
}

/// Remove sub-paths that have no drawing commands (only MoveTo or empty).
pub fn remove_empty_subpaths(path: &mut VecPath) {
    path.subpaths.retain(|sp| {
        sp.commands
            .iter()
            .any(|c| !matches!(c, PathCommand::MoveTo { .. }))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_zero_length_filters_duplicate_lines() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L10 10 L20 20");
        remove_zero_length_segments(&mut path);
        // Should remove the duplicate L10 10
        assert_eq!(path.subpaths[0].commands.len(), 3);
    }

    #[test]
    fn dedup_removes_close_points() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L10.001 10.001 L20 20");
        dedup_consecutive_points(&mut path, 0.01);
        assert_eq!(path.subpaths[0].commands.len(), 3);
    }

    #[test]
    fn remove_empty_subpaths_filters() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 M20 20");
        // Second subpath only has MoveTo — should be removed
        remove_empty_subpaths(&mut path);
        assert_eq!(path.subpaths.len(), 1);
    }

    #[test]
    fn cleanup_preserves_close_commands() {
        let mut path = VecPath::parse_svg_d("M0 0 L10 10 L20 0 Z");
        remove_zero_length_segments(&mut path);
        assert!(
            path.subpaths[0]
                .commands
                .iter()
                .any(|c| matches!(c, PathCommand::Close))
        );
    }
}
