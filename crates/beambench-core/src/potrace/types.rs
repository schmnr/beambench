// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

/// 2D point used throughout potrace internals.
#[derive(Clone, Copy, Debug, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// Cumulative sums for O(1) penalty computation in polygon fitting.
#[derive(Clone, Debug, Default)]
pub struct Sums {
    pub x: f64,
    pub y: f64,
    pub x2: f64,
    pub xy: f64,
    pub y2: f64,
}

/// Segment tag — corner (sharp vertex) or curve (smooth Bezier).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentTag {
    Corner,
    Curve,
}

/// A single segment of a fitted curve.
/// `c[0]`, `c[1]` are control points; `c[2]` is the endpoint.
/// For corners: `c[1]` = vertex, `c[2]` = endpoint.
/// For curves: `c[0]` = first control, `c[1]` = second control, `c[2]` = endpoint.
#[derive(Clone, Debug)]
pub struct Segment {
    pub tag: SegmentTag,
    pub c: [Point; 3],
    pub vertex: Point,
    pub alpha: f64,
    pub alpha0: f64,
    pub beta: f64,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            tag: SegmentTag::Corner,
            c: [Point::default(); 3],
            vertex: Point::default(),
            alpha: 0.0,
            alpha0: 0.0,
            beta: 0.0,
        }
    }
}

/// Internal curve: a sequence of segments forming a closed contour.
#[derive(Clone, Debug)]
pub struct InternalCurve {
    pub segments: Vec<Segment>,
    pub alphacurve: bool,
}

impl InternalCurve {
    pub fn new(m: usize) -> Self {
        Self {
            segments: (0..m).map(|_| Segment::default()).collect(),
            alphacurve: false,
        }
    }

    pub fn n(&self) -> usize {
        self.segments.len()
    }
}

/// Turn policy for resolving ambiguous pixel boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TurnPolicy {
    Black,
    White,
    Left,
    Right,
    Minority,
    Majority,
    Random,
}

impl Default for TurnPolicy {
    fn default() -> Self {
        TurnPolicy::Minority
    }
}

/// A traced path from bitmap decomposition.
/// Contains the raw boundary points and all intermediate computation results.
pub struct InternalPath {
    /// Boundary vertices (integer pixel coordinates).
    pub pt: Vec<Point>,
    /// Signed area (positive = CW, negative = CCW).
    pub area: i64,
    /// true = path encloses foreground ("+" path), false = hole ("-" path).
    pub sign: bool,
    // Phase 2 intermediates
    pub lon: Vec<usize>,
    pub x0: f64,
    pub y0: f64,
    pub sums: Vec<Sums>,
    pub m: usize,
    pub po: Vec<usize>,
    // Phase 3-5 results
    pub curve: Option<InternalCurve>,
    pub ocurve: Option<InternalCurve>,
    /// Final curve (either ocurve if optimization ran, else curve).
    pub fcurve: Option<InternalCurve>,
}

impl InternalPath {
    pub fn new(pt: Vec<Point>, area: i64, sign: bool) -> Self {
        let n = pt.len();
        Self {
            pt,
            area,
            sign,
            lon: vec![0; n],
            x0: 0.0,
            y0: 0.0,
            sums: Vec::new(),
            m: 0,
            po: Vec::new(),
            curve: None,
            ocurve: None,
            fcurve: None,
        }
    }

    pub fn len(&self) -> usize {
        self.pt.len()
    }
}

/// Optimization result for a single segment merge attempt.
pub struct OptiResult {
    pub pen: f64,
    pub c: [Point; 2],
    pub t: f64,
    pub s: f64,
    pub alpha: f64,
}

impl Default for OptiResult {
    fn default() -> Self {
        Self {
            pen: 0.0,
            c: [Point::default(); 2],
            t: 0.0,
            s: 0.0,
            alpha: 0.0,
        }
    }
}
