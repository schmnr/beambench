// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

#![allow(dead_code)]

use super::types::Point;

/// Deterministic pseudo-random hash for turnpolicy.
/// The lookup table comes from Peter Selinger's original potrace C source.
const DETRAND_TABLE: [u8; 256] = [
    0, 1, 1, 0, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 1, 1, 0, 0, 0, 1, 1, 1, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0,
    0, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1,
    1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 1, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 1, 1, 1, 0, 0, 0,
    0, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 1, 0,
    1, 1, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0,
    0, 0, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 0, 0,
    0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 1, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 1, 1, 0, 0, 1, 1, 0,
];

pub fn detrand(x: i32, y: i32) -> bool {
    let z = (0x04B3E375u32.wrapping_mul(x as u32) ^ (y as u32)).wrapping_mul(0x05A8EF93);
    let r = DETRAND_TABLE[(z & 0xFF) as usize]
        ^ DETRAND_TABLE[((z >> 8) & 0xFF) as usize]
        ^ DETRAND_TABLE[((z >> 16) & 0xFF) as usize]
        ^ DETRAND_TABLE[((z >> 24) & 0xFF) as usize];
    r != 0
}

/// Modulo that works correctly for negative values (Python-style).
pub fn modulo(a: i32, n: i32) -> usize {
    ((a % n + n) % n) as usize
}

/// Floor division for negative values.
pub fn floordiv(a: i64, n: i64) -> i64 {
    if a >= 0 { a / n } else { -1 - (-1 - a) / n }
}

/// Sign of a value: 1, -1, or 0.
pub fn sign(x: f64) -> i32 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}

/// Linear interpolation: a + t*(b - a).
pub fn interval(t: f64, a: &Point, b: &Point) -> Point {
    Point::new(a.x + t * (b.x - a.x), a.y + t * (b.y - a.y))
}

/// Cross product (p1-p0) x (p2-p0). Area of parallelogram.
pub fn dpara(p0: &Point, p1: &Point, p2: &Point) -> f64 {
    (p1.x - p0.x) * (p2.y - p0.y) - (p2.x - p0.x) * (p1.y - p0.y)
}

/// Direction 90 degrees CCW from (p2 - p0), snapped to {-1, 0, 1}.
pub fn dorth_infty(p0: &Point, p2: &Point) -> Point {
    Point::new(-sign(p2.y - p0.y) as f64, sign(p2.x - p0.x) as f64)
}

/// Helper for intersection testing.
pub fn ddenom(p0: &Point, p2: &Point) -> f64 {
    let r = dorth_infty(p0, p2);
    r.y * (p2.x - p0.x) - r.x * (p2.y - p0.y)
}

/// Cross product (p1-p0) x (p3-p2).
pub fn cprod(p0: &Point, p1: &Point, p2: &Point, p3: &Point) -> f64 {
    (p1.x - p0.x) * (p3.y - p2.y) - (p3.x - p2.x) * (p1.y - p0.y)
}

/// Dot product (p1-p0) . (p2-p0).
pub fn iprod(p0: &Point, p1: &Point, p2: &Point) -> f64 {
    (p1.x - p0.x) * (p2.x - p0.x) + (p1.y - p0.y) * (p2.y - p0.y)
}

/// Dot product (p1-p0) . (p3-p2).
pub fn iprod1(p0: &Point, p1: &Point, p2: &Point, p3: &Point) -> f64 {
    (p1.x - p0.x) * (p3.x - p2.x) + (p1.y - p0.y) * (p3.y - p2.y)
}

/// Distance between two points.
pub fn ddist(p: &Point, q: &Point) -> f64 {
    ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt()
}

/// 2D cross product of two vectors given as (x, y) pairs.
pub fn xprod(p1x: f64, p1y: f64, p2x: f64, p2y: f64) -> f64 {
    p1x * p2y - p1y * p2x
}

/// Evaluate cubic Bezier curve at parameter t.
pub fn bezier(t: f64, p0: &Point, p1: &Point, p2: &Point, p3: &Point) -> Point {
    let s = 1.0 - t;
    Point::new(
        s * s * s * p0.x + 3.0 * s * s * t * p1.x + 3.0 * t * t * s * p2.x + t * t * t * p3.x,
        s * s * s * p0.y + 3.0 * s * s * t * p1.y + 3.0 * t * t * s * p2.y + t * t * t * p3.y,
    )
}

/// Find parameter t in [0,1] where Bezier is tangent to line (q0, q1).
/// Returns -1.0 if no valid tangent found.
pub fn tangent(p0: &Point, p1: &Point, p2: &Point, p3: &Point, q0: &Point, q1: &Point) -> f64 {
    let a_val = cprod(p0, p1, q0, q1);
    let b_val = cprod(p1, p2, q0, q1);
    let c_val = cprod(p2, p3, q0, q1);

    let a = a_val - 2.0 * b_val + c_val;
    let b = -2.0 * a_val + 2.0 * b_val;
    let c = a_val;

    if a == 0.0 {
        return -1.0;
    }

    let d = b * b - 4.0 * a * c;
    if d < 0.0 {
        return -1.0;
    }

    let s = d.sqrt();
    let r1 = (-b + s) / (2.0 * a);
    let r2 = (-b - s) / (2.0 * a);

    if (0.0..=1.0).contains(&r1) {
        r1
    } else if (0.0..=1.0).contains(&r2) {
        r2
    } else {
        -1.0
    }
}

/// Apply quadratic form Q (3x3 matrix) to vector (w.x, w.y, 1).
pub fn quadform(q: &[[f64; 3]; 3], w: &Point) -> f64 {
    let v = [w.x, w.y, 1.0];
    let mut sum = 0.0;
    for i in 0..3 {
        for j in 0..3 {
            sum += v[i] * q[i][j] * v[j];
        }
    }
    sum
}

/// Test if b is between a and c in cyclic order.
pub fn cyclic(a: usize, b: usize, c: usize) -> bool {
    if a <= c {
        a <= b && b < c
    } else {
        a <= b || b < c
    }
}

/// cos(179 degrees) — used for near-antiparallel check in optimization.
pub const COS179: f64 = -0.999_847_695;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::potrace::types::Point;

    #[test]
    fn test_dpara() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(1.0, 0.0);
        let p2 = Point::new(0.0, 1.0);
        assert!((dpara(&p0, &p1, &p2) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_interval() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(10.0, 10.0);
        let mid = interval(0.5, &a, &b);
        assert!((mid.x - 5.0).abs() < 1e-10);
        assert!((mid.y - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_bezier_endpoints() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(1.0, 2.0);
        let p2 = Point::new(3.0, 2.0);
        let p3 = Point::new(4.0, 0.0);
        let start = bezier(0.0, &p0, &p1, &p2, &p3);
        let end = bezier(1.0, &p0, &p1, &p2, &p3);
        assert!((start.x - p0.x).abs() < 1e-10);
        assert!((end.x - p3.x).abs() < 1e-10);
    }

    #[test]
    fn test_mod_negative() {
        assert_eq!(modulo(-1, 5), 4);
        assert_eq!(modulo(5, 5), 0);
        assert_eq!(modulo(3, 5), 3);
    }

    #[test]
    fn test_cyclic() {
        assert!(cyclic(2, 3, 5));
        assert!(!cyclic(2, 5, 3));
        assert!(cyclic(5, 6, 3));
    }

    #[test]
    fn test_sign_fn() {
        assert_eq!(sign(5.0), 1);
        assert_eq!(sign(-3.0), -1);
        assert_eq!(sign(0.0), 0);
    }

    #[test]
    fn test_quadform() {
        let q = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let w = Point::new(3.0, 4.0);
        let result = quadform(&q, &w);
        assert!((result - 26.0).abs() < 1e-10);
    }

    #[test]
    fn test_floordiv() {
        assert_eq!(floordiv(7, 3), 2);
        assert_eq!(floordiv(-7, 3), -3);
        assert_eq!(floordiv(-1, 3), -1);
        assert_eq!(floordiv(0, 3), 0);
    }

    #[test]
    fn test_xprod() {
        assert!((xprod(1.0, 0.0, 0.0, 1.0) - 1.0).abs() < 1e-10);
        assert!((xprod(0.0, 1.0, 1.0, 0.0) - (-1.0)).abs() < 1e-10);
    }

    #[test]
    fn test_ddist() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert!((ddist(&a, &b) - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_cprod() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(1.0, 0.0);
        let p2 = Point::new(0.0, 0.0);
        let p3 = Point::new(0.0, 1.0);
        assert!((cprod(&p0, &p1, &p2, &p3) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_iprod() {
        let p0 = Point::new(0.0, 0.0);
        let p1 = Point::new(1.0, 0.0);
        let p2 = Point::new(1.0, 0.0);
        assert!((iprod(&p0, &p1, &p2) - 1.0).abs() < 1e-10);
    }
}
