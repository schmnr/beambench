use serde::{Deserialize, Serialize};

use crate::geometry::{Bounds, Point2D};

/// A single path command in a vector path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PathCommand {
    MoveTo {
        x: f64,
        y: f64,
    },
    LineTo {
        x: f64,
        y: f64,
    },
    QuadTo {
        cx: f64,
        cy: f64,
        x: f64,
        y: f64,
    },
    CubicTo {
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    },
    Close,
}

/// A contiguous sub-path (sequence of commands starting with MoveTo).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubPath {
    pub commands: Vec<PathCommand>,
    pub closed: bool,
}

impl SubPath {
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            closed: false,
        }
    }
}

impl Default for SubPath {
    fn default() -> Self {
        Self::new()
    }
}

/// A structured vector path consisting of one or more sub-paths.
/// This is the in-memory manipulation format; `path_data: String` (SVG d-string)
/// remains the serialization/storage format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VecPath {
    pub subpaths: Vec<SubPath>,
}

impl VecPath {
    pub fn new() -> Self {
        Self {
            subpaths: Vec::new(),
        }
    }

    /// Parse an SVG d-string into a structured VecPath.
    pub fn parse_svg_d(d: &str) -> Self {
        let mut path = VecPath::new();
        let mut current_subpath = SubPath::new();
        let mut has_content = false;

        let tokens = tokenize_svg_d(d);
        let mut i = 0;

        while i < tokens.len() {
            match tokens[i].as_str() {
                "M" | "m" => {
                    let is_relative = tokens[i] == "m";
                    if has_content && !current_subpath.commands.is_empty() {
                        path.subpaths.push(current_subpath);
                        current_subpath = SubPath::new();
                    }
                    if i + 2 < tokens.len() {
                        let (mut x, mut y) = (parse_f64(&tokens[i + 1]), parse_f64(&tokens[i + 2]));
                        if is_relative && let Some(prev) = last_point(&path, &current_subpath) {
                            x += prev.0;
                            y += prev.1;
                        }
                        current_subpath.commands.push(PathCommand::MoveTo { x, y });
                        has_content = true;
                        i += 3;
                    } else {
                        i += 1;
                    }
                }
                "L" | "l" => {
                    let is_relative = tokens[i] == "l";
                    if i + 2 < tokens.len() {
                        let (mut x, mut y) = (parse_f64(&tokens[i + 1]), parse_f64(&tokens[i + 2]));
                        if is_relative && let Some(prev) = last_point(&path, &current_subpath) {
                            x += prev.0;
                            y += prev.1;
                        }
                        current_subpath.commands.push(PathCommand::LineTo { x, y });
                        i += 3;
                    } else {
                        i += 1;
                    }
                }
                "H" | "h" => {
                    let is_relative = tokens[i] == "h";
                    if i + 1 < tokens.len() {
                        let mut x = parse_f64(&tokens[i + 1]);
                        let y;
                        if let Some(prev) = last_point(&path, &current_subpath) {
                            y = prev.1;
                            if is_relative {
                                x += prev.0;
                            }
                        } else {
                            y = 0.0;
                        }
                        current_subpath.commands.push(PathCommand::LineTo { x, y });
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "V" | "v" => {
                    let is_relative = tokens[i] == "v";
                    if i + 1 < tokens.len() {
                        let x;
                        let mut y = parse_f64(&tokens[i + 1]);
                        if let Some(prev) = last_point(&path, &current_subpath) {
                            x = prev.0;
                            if is_relative {
                                y += prev.1;
                            }
                        } else {
                            x = 0.0;
                        }
                        current_subpath.commands.push(PathCommand::LineTo { x, y });
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "Q" | "q" => {
                    let is_relative = tokens[i] == "q";
                    if i + 4 < tokens.len() {
                        let (mut cx, mut cy) =
                            (parse_f64(&tokens[i + 1]), parse_f64(&tokens[i + 2]));
                        let (mut x, mut y) = (parse_f64(&tokens[i + 3]), parse_f64(&tokens[i + 4]));
                        if is_relative && let Some(prev) = last_point(&path, &current_subpath) {
                            cx += prev.0;
                            cy += prev.1;
                            x += prev.0;
                            y += prev.1;
                        }
                        current_subpath
                            .commands
                            .push(PathCommand::QuadTo { cx, cy, x, y });
                        i += 5;
                    } else {
                        i += 1;
                    }
                }
                "C" | "c" => {
                    let is_relative = tokens[i] == "c";
                    if i + 6 < tokens.len() {
                        let (mut c1x, mut c1y) =
                            (parse_f64(&tokens[i + 1]), parse_f64(&tokens[i + 2]));
                        let (mut c2x, mut c2y) =
                            (parse_f64(&tokens[i + 3]), parse_f64(&tokens[i + 4]));
                        let (mut x, mut y) = (parse_f64(&tokens[i + 5]), parse_f64(&tokens[i + 6]));
                        if is_relative && let Some(prev) = last_point(&path, &current_subpath) {
                            c1x += prev.0;
                            c1y += prev.1;
                            c2x += prev.0;
                            c2y += prev.1;
                            x += prev.0;
                            y += prev.1;
                        }
                        current_subpath.commands.push(PathCommand::CubicTo {
                            c1x,
                            c1y,
                            c2x,
                            c2y,
                            x,
                            y,
                        });
                        i += 7;
                    } else {
                        i += 1;
                    }
                }
                "Z" | "z" => {
                    current_subpath.commands.push(PathCommand::Close);
                    current_subpath.closed = true;
                    i += 1;
                }
                _ => {
                    // Skip unknown tokens
                    i += 1;
                }
            }
        }

        if !current_subpath.commands.is_empty() {
            path.subpaths.push(current_subpath);
        }

        path
    }

    /// Serialize this VecPath to an SVG d-string.
    pub fn to_svg_d(&self) -> String {
        use std::fmt::Write;
        let mut d = String::new();
        for subpath in &self.subpaths {
            for cmd in &subpath.commands {
                if !d.is_empty() && !matches!(cmd, PathCommand::Close) && !d.ends_with(' ') {
                    d.push(' ');
                }
                match cmd {
                    PathCommand::MoveTo { x, y } => {
                        let _ = write!(d, "M{} {}", format_coord(*x), format_coord(*y));
                    }
                    PathCommand::LineTo { x, y } => {
                        let _ = write!(d, "L{} {}", format_coord(*x), format_coord(*y));
                    }
                    PathCommand::QuadTo { cx, cy, x, y } => {
                        let _ = write!(
                            d,
                            "Q{} {} {} {}",
                            format_coord(*cx),
                            format_coord(*cy),
                            format_coord(*x),
                            format_coord(*y)
                        );
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => {
                        let _ = write!(
                            d,
                            "C{} {} {} {} {} {}",
                            format_coord(*c1x),
                            format_coord(*c1y),
                            format_coord(*c2x),
                            format_coord(*c2y),
                            format_coord(*x),
                            format_coord(*y)
                        );
                    }
                    PathCommand::Close => {
                        d.push('Z');
                    }
                }
            }
        }
        d
    }

    /// Compute the axis-aligned bounding box of this path.
    /// Returns `None` if the path has no points.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut has_points = false;

        for subpath in &self.subpaths {
            for cmd in &subpath.commands {
                let points = cmd_points(cmd);
                for (x, y) in points {
                    has_points = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        if has_points {
            Some(Bounds::new(
                Point2D::new(min_x, min_y),
                Point2D::new(max_x, max_y),
            ))
        } else {
            None
        }
    }

    /// Curve-sampled bounds matching frontend's computePathBBox().
    /// Use for obj.bounds and coordinate-mapping contexts.
    /// Quadratic curves: 32 sample steps. Cubic curves: 48 sample steps.
    pub fn visual_bounds(&self) -> Option<Bounds> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut has_points = false;

        let mut include = |x: f64, y: f64| {
            has_points = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        };

        for subpath in &self.subpaths {
            let mut curr_x = 0.0;
            let mut curr_y = 0.0;
            let mut start_x = 0.0;
            let mut start_y = 0.0;

            for cmd in &subpath.commands {
                match *cmd {
                    PathCommand::MoveTo { x, y } => {
                        include(x, y);
                        curr_x = x;
                        curr_y = y;
                        start_x = x;
                        start_y = y;
                    }
                    PathCommand::LineTo { x, y } => {
                        include(x, y);
                        curr_x = x;
                        curr_y = y;
                    }
                    PathCommand::QuadTo { cx, cy, x, y } => {
                        let steps = 32;
                        for i in 0..=steps {
                            let t = i as f64 / steps as f64;
                            let mt = 1.0 - t;
                            let sx = mt * mt * curr_x + 2.0 * mt * t * cx + t * t * x;
                            let sy = mt * mt * curr_y + 2.0 * mt * t * cy + t * t * y;
                            include(sx, sy);
                        }
                        curr_x = x;
                        curr_y = y;
                    }
                    PathCommand::CubicTo {
                        c1x,
                        c1y,
                        c2x,
                        c2y,
                        x,
                        y,
                    } => {
                        let steps = 48;
                        for i in 0..=steps {
                            let t = i as f64 / steps as f64;
                            let mt = 1.0 - t;
                            let sx = mt * mt * mt * curr_x
                                + 3.0 * mt * mt * t * c1x
                                + 3.0 * mt * t * t * c2x
                                + t * t * t * x;
                            let sy = mt * mt * mt * curr_y
                                + 3.0 * mt * mt * t * c1y
                                + 3.0 * mt * t * t * c2y
                                + t * t * t * y;
                            include(sx, sy);
                        }
                        curr_x = x;
                        curr_y = y;
                    }
                    PathCommand::Close => {
                        include(curr_x, curr_y);
                        include(start_x, start_y);
                        curr_x = start_x;
                        curr_y = start_y;
                    }
                }
            }
        }

        if has_points {
            Some(Bounds::new(
                Point2D::new(min_x, min_y),
                Point2D::new(max_x, max_y),
            ))
        } else {
            None
        }
    }

    /// Returns true if this path has no sub-paths or no commands.
    pub fn is_empty(&self) -> bool {
        self.subpaths.is_empty() || self.subpaths.iter().all(|sp| sp.commands.is_empty())
    }

    /// Total number of commands across all sub-paths.
    pub fn command_count(&self) -> usize {
        self.subpaths.iter().map(|sp| sp.commands.len()).sum()
    }
}

impl Default for VecPath {
    fn default() -> Self {
        Self::new()
    }
}

/// A polyline: a sequence of points with optional closure.
/// This is the planner-ready output format after flattening bezier curves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Polyline {
    pub points: Vec<Point2D>,
    pub closed: bool,
}

impl Polyline {
    pub fn new(points: Vec<Point2D>, closed: bool) -> Self {
        Self { points, closed }
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }
}

// --- Helpers ---

/// Extract all endpoint coordinates from a command (for bounding box).
fn cmd_points(cmd: &PathCommand) -> Vec<(f64, f64)> {
    match *cmd {
        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => vec![(x, y)],
        PathCommand::QuadTo { cx, cy, x, y } => vec![(cx, cy), (x, y)],
        PathCommand::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => vec![(c1x, c1y), (c2x, c2y), (x, y)],
        PathCommand::Close => vec![],
    }
}

/// Get the last endpoint from the current context.
fn last_point(path: &VecPath, current: &SubPath) -> Option<(f64, f64)> {
    // Check current sub-path first
    for cmd in current.commands.iter().rev() {
        match *cmd {
            PathCommand::MoveTo { x, y }
            | PathCommand::LineTo { x, y }
            | PathCommand::QuadTo { x, y, .. }
            | PathCommand::CubicTo { x, y, .. } => return Some((x, y)),
            PathCommand::Close => {
                // On close, current point returns to the sub-path's start
                for c in &current.commands {
                    if let PathCommand::MoveTo { x, y } = c {
                        return Some((*x, *y));
                    }
                }
            }
        }
    }
    // Check previous sub-paths
    for sp in path.subpaths.iter().rev() {
        for cmd in sp.commands.iter().rev() {
            match *cmd {
                PathCommand::MoveTo { x, y }
                | PathCommand::LineTo { x, y }
                | PathCommand::QuadTo { x, y, .. }
                | PathCommand::CubicTo { x, y, .. } => return Some((x, y)),
                PathCommand::Close => {
                    // After close, current point is at the sub-path's start
                    for c in &sp.commands {
                        if let PathCommand::MoveTo { x, y } = c {
                            return Some((*x, *y));
                        }
                    }
                    continue;
                }
            }
        }
    }
    None
}

/// Tokenize an SVG d-string into commands and numbers.
fn tokenize_svg_d(d: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();

    for ch in d.chars() {
        if (ch == 'e' || ch == 'E')
            && current
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_digit() || c == '.')
        {
            // Exponent marker inside a number ('e'/'E' is never a path
            // command, only scientific notation like "1e-3").
            current.push(ch);
        } else if ch.is_ascii_alphabetic() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            tokens.push(ch.to_string());
        } else if ch == ',' || ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else if (ch == '-' || ch == '+')
            && !current.is_empty()
            && !current.ends_with('e')
            && !current.ends_with('E')
        {
            // Sign starts a new number (unless directly after an exponent marker)
            tokens.push(current.clone());
            current.clear();
            current.push(ch);
        } else if ch == '.'
            && (current.contains('.') || current.contains('e') || current.contains('E'))
        {
            // Per SVG spec, a second '.' while the current number already has a
            // fractional part (e.g. "1.5.3" = "1.5 0.3"), or a '.' after the
            // exponent marker, terminates the number and starts a new one.
            tokens.push(current.clone());
            current.clear();
            current.push(ch);
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

fn parse_f64(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(0.0)
}

fn format_coord(v: f64) -> String {
    if v == v.floor() && v.abs() < 1e12 {
        format!("{}", v as i64)
    } else {
        // Use enough precision for roundtrip fidelity
        let s = format!("{:.6}", v);
        // Trim trailing zeros after decimal point
        let s = s.trim_end_matches('0');
        let s = s.trim_end_matches('.');
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_move_line_close() {
        let path = VecPath::parse_svg_d("M10 20 L30 40 Z");
        assert_eq!(path.subpaths.len(), 1);
        assert_eq!(path.subpaths[0].commands.len(), 3);
        assert!(path.subpaths[0].closed);
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 10.0, y: 20.0 }
        );
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x: 30.0, y: 40.0 }
        );
        assert_eq!(path.subpaths[0].commands[2], PathCommand::Close);
    }

    #[test]
    fn parse_compact_decimals() {
        // Per SVG spec, "1.5.3" is two numbers: 1.5 and 0.3.
        let path = VecPath::parse_svg_d("M1.5.3L2.5.5");
        assert_eq!(path.subpaths.len(), 1);
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 1.5, y: 0.3 }
        );
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x: 2.5, y: 0.5 }
        );
    }

    #[test]
    fn parse_spaced_decimals_unchanged() {
        let path = VecPath::parse_svg_d("M 1.5 0.3");
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 1.5, y: 0.3 }
        );
    }

    #[test]
    fn parse_compact_negative() {
        let path = VecPath::parse_svg_d("M1.5-2.3");
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 1.5, y: -2.3 }
        );
    }

    #[test]
    fn parse_exponent_notation() {
        // '-' or '+' directly after the exponent marker stays in the number.
        let path = VecPath::parse_svg_d("M1e-3 2E+2");
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 0.001, y: 200.0 }
        );
    }

    #[test]
    fn parse_dot_after_exponent_starts_new_number() {
        // "1e2.5" is 1e2 followed by 0.5.
        let path = VecPath::parse_svg_d("M1e2.5");
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: 100.0, y: 0.5 }
        );
    }

    #[test]
    fn parse_cubic() {
        let path = VecPath::parse_svg_d("M0 0 C10 20 30 40 50 60");
        assert_eq!(path.subpaths.len(), 1);
        assert_eq!(path.subpaths[0].commands.len(), 2);
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::CubicTo {
                c1x: 10.0,
                c1y: 20.0,
                c2x: 30.0,
                c2y: 40.0,
                x: 50.0,
                y: 60.0,
            }
        );
    }

    #[test]
    fn parse_quad() {
        let path = VecPath::parse_svg_d("M0 0 Q10 20 30 40");
        assert_eq!(path.subpaths[0].commands.len(), 2);
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::QuadTo {
                cx: 10.0,
                cy: 20.0,
                x: 30.0,
                y: 40.0,
            }
        );
    }

    #[test]
    fn roundtrip_parse_serialize() {
        let original = "M10 20 L30 40 C50 60 70 80 90 100 Q110 120 130 140 Z";
        let path = VecPath::parse_svg_d(original);
        let serialized = path.to_svg_d();
        let reparsed = VecPath::parse_svg_d(&serialized);
        assert_eq!(path, reparsed);
    }

    #[test]
    fn bounds_computation() {
        let path = VecPath::parse_svg_d("M10 20 L30 5 L50 40");
        let b = path.bounds().unwrap();
        assert_eq!(b.min.x, 10.0);
        assert_eq!(b.min.y, 5.0);
        assert_eq!(b.max.x, 50.0);
        assert_eq!(b.max.y, 40.0);
    }

    #[test]
    fn empty_path_has_no_bounds() {
        let path = VecPath::new();
        assert!(path.bounds().is_none());
        assert!(path.is_empty());
    }

    #[test]
    fn multiple_subpaths() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 Z M20 20 L30 30");
        assert_eq!(path.subpaths.len(), 2);
        assert!(path.subpaths[0].closed);
        assert!(!path.subpaths[1].closed);
    }

    #[test]
    fn polyline_basics() {
        let poly = Polyline::new(
            vec![Point2D::new(0.0, 0.0), Point2D::new(10.0, 10.0)],
            false,
        );
        assert_eq!(poly.len(), 2);
        assert!(!poly.is_empty());
        assert!(!poly.closed);
    }

    #[test]
    fn parse_negative_coordinates() {
        let path = VecPath::parse_svg_d("M-10-20 L-30 -40");
        assert_eq!(
            path.subpaths[0].commands[0],
            PathCommand::MoveTo { x: -10.0, y: -20.0 }
        );
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x: -30.0, y: -40.0 }
        );
    }

    #[test]
    fn parse_h_and_v_commands() {
        let path = VecPath::parse_svg_d("M10 20 H30 V40");
        assert_eq!(path.subpaths[0].commands.len(), 3);
        assert_eq!(
            path.subpaths[0].commands[1],
            PathCommand::LineTo { x: 30.0, y: 20.0 }
        );
        assert_eq!(
            path.subpaths[0].commands[2],
            PathCommand::LineTo { x: 30.0, y: 40.0 }
        );
    }

    #[test]
    fn command_count() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 L20 20 Z");
        assert_eq!(path.command_count(), 4);
    }

    #[test]
    fn relative_move_after_close_uses_subpath_start() {
        // "M10 20 L30 40 Z m5 5 L50 60"
        // After Z, current point is at (10, 20) (the sub-path's MoveTo).
        // Relative m5 5 should produce M15 25, not M35 45.
        let path = VecPath::parse_svg_d("M10 20 L30 40 Z m5 5 L50 60");
        assert_eq!(path.subpaths.len(), 2);
        assert_eq!(
            path.subpaths[1].commands[0],
            PathCommand::MoveTo { x: 15.0, y: 25.0 }
        );
    }

    #[test]
    fn vecpath_serde_roundtrip() {
        let path = VecPath::parse_svg_d("M0 0 L10 10 C20 20 30 30 40 40 Z");
        let json = serde_json::to_string(&path).unwrap();
        let restored: VecPath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, restored);
    }

    #[test]
    fn visual_bounds_cubic_tighter_than_control_hull() {
        // Same cubic as frontend drawObjects.test.ts: M 0 0 C 50 100 100 100 100 0
        // Control hull maxY = 100, but curve peak is around y≈75
        let path = VecPath::parse_svg_d("M0 0 C50 100 100 100 100 0");
        let vis = path.visual_bounds().unwrap();
        let hull = path.bounds().unwrap();
        assert_eq!(hull.max.y, 100.0);
        assert!(
            vis.max.y > 70.0,
            "curve does bulge, vis.max.y={}",
            vis.max.y
        );
        assert!(
            vis.max.y < 80.0,
            "visual should be tighter than hull, vis.max.y={}",
            vis.max.y
        );
    }

    #[test]
    fn visual_bounds_matches_bounds_for_line_only_paths() {
        let path = VecPath::parse_svg_d("M10 20 L50 30 L30 60");
        let vis = path.visual_bounds().unwrap();
        let hull = path.bounds().unwrap();
        assert!((vis.min.x - hull.min.x).abs() < 1e-9);
        assert!((vis.min.y - hull.min.y).abs() < 1e-9);
        assert!((vis.max.x - hull.max.x).abs() < 1e-9);
        assert!((vis.max.y - hull.max.y).abs() < 1e-9);
    }

    #[test]
    fn visual_bounds_none_for_empty() {
        let path = VecPath::new();
        assert!(path.visual_bounds().is_none());
    }

    #[test]
    fn visual_bounds_handles_extended_bezier_handles() {
        // Extended control points: C0 500 100 500 → hull maxY=500, visual much less
        let path = VecPath::parse_svg_d("M0 0 C0 500 100 500 100 0");
        let vis = path.visual_bounds().unwrap();
        let hull = path.bounds().unwrap();
        assert_eq!(hull.max.y, 500.0);
        assert!(
            vis.max.y < 400.0,
            "visual maxY={} should be < 400",
            vis.max.y
        );
        assert!(
            vis.max.y > 300.0,
            "curve does bulge significantly, vis.max.y={}",
            vis.max.y
        );
    }
}
