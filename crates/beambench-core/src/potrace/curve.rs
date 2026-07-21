// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

#![allow(dead_code)]

use super::math_utils::{
    COS179, bezier, cprod, ddenom, ddist, dpara, interval, iprod, iprod1, modulo, sign, tangent,
};
use super::types::{InternalCurve, InternalPath, OptiResult, SegmentTag};

fn sq(x: f64) -> f64 {
    x * x
}

/// Reverse the vertex order of a curve in place.
pub fn reverse(curve: &mut InternalCurve) {
    let m = curve.n();
    if m == 0 {
        return;
    }
    let mut i = 0;
    let mut j = m - 1;
    while i < j {
        let tmp = curve.segments[i].vertex;
        curve.segments[i].vertex = curve.segments[j].vertex;
        curve.segments[j].vertex = tmp;
        i += 1;
        j -= 1;
    }
}

/// Fit Bezier curves to a polygon's adjusted vertices.
pub fn smooth(curve: &mut InternalCurve, alphamax: f64) {
    let m = curve.n();
    for i in 0..m {
        let j = modulo((i + 1) as i32, m as i32);
        let k = modulo((i + 2) as i32, m as i32);

        let p4 = interval(0.5, &curve.segments[k].vertex, &curve.segments[j].vertex);

        let denom = ddenom(&curve.segments[i].vertex, &curve.segments[k].vertex);
        let alpha;
        if denom != 0.0 {
            let dd = dpara(
                &curve.segments[i].vertex,
                &curve.segments[j].vertex,
                &curve.segments[k].vertex,
            ) / denom;
            let dd = dd.abs();
            alpha = if dd > 1.0 {
                (1.0 - 1.0 / dd) / 0.75
            } else {
                0.0 / 0.75
            };
        } else {
            alpha = 4.0 / 3.0;
        }

        curve.segments[j].alpha0 = alpha;

        if alpha >= alphamax {
            curve.segments[j].tag = SegmentTag::Corner;
            curve.segments[j].c[1] = curve.segments[j].vertex;
            curve.segments[j].c[2] = p4;
        } else {
            let a = if alpha < 0.55 {
                0.55
            } else if alpha > 1.0 {
                1.0
            } else {
                alpha
            };
            let p2 = interval(
                0.5 + 0.5 * a,
                &curve.segments[i].vertex,
                &curve.segments[j].vertex,
            );
            let p3 = interval(
                0.5 + 0.5 * a,
                &curve.segments[k].vertex,
                &curve.segments[j].vertex,
            );
            curve.segments[j].tag = SegmentTag::Curve;
            curve.segments[j].c[0] = p2;
            curve.segments[j].c[1] = p3;
            curve.segments[j].c[2] = p4;
        }
        curve.segments[j].alpha = alpha;
        curve.segments[j].beta = 0.5;
    }
    curve.alphacurve = true;
}

/// Check if segments i..j can be merged into a single optimized segment.
/// Returns None if merge fails, Some(OptiResult) if valid.
fn opti_penalty(
    path: &InternalPath,
    i: usize,
    j: usize,
    opttolerance: f64,
    convc: &[i32],
    areac: &[f64],
) -> Option<OptiResult> {
    let curve = path.curve.as_ref().unwrap();
    let m = curve.n();

    if i == j {
        return None;
    }

    let i1 = modulo((i + 1) as i32, m as i32);
    let k1 = modulo((i + 1) as i32, m as i32);
    let conv = convc[k1];
    if conv == 0 {
        return None;
    }

    let d = ddist(&curve.segments[i].vertex, &curve.segments[i1].vertex);

    // Check convexity consistency
    let mut k = k1;
    while k != j {
        let kk1 = modulo((k + 1) as i32, m as i32);
        let kk2 = modulo((k + 2) as i32, m as i32);
        if convc[kk1] != conv {
            return None;
        }
        if sign(cprod(
            &curve.segments[i].vertex,
            &curve.segments[i1].vertex,
            &curve.segments[kk1].vertex,
            &curve.segments[kk2].vertex,
        )) != conv
        {
            return None;
        }
        if iprod1(
            &curve.segments[i].vertex,
            &curve.segments[i1].vertex,
            &curve.segments[kk1].vertex,
            &curve.segments[kk2].vertex,
        ) < d * ddist(&curve.segments[kk1].vertex, &curve.segments[kk2].vertex) * COS179
        {
            return None;
        }
        k = kk1;
    }

    let p0 = curve.segments[modulo(i as i32, m as i32)].c[2];
    let p1 = curve.segments[modulo((i + 1) as i32, m as i32)].vertex;
    let p2 = curve.segments[modulo(j as i32, m as i32)].vertex;
    let p3 = curve.segments[modulo(j as i32, m as i32)].c[2];

    let mut area = areac[j] - areac[i];
    area -= dpara(
        &curve.segments[0].vertex,
        &curve.segments[i].c[2],
        &curve.segments[j].c[2],
    ) / 2.0;
    if i >= j {
        area += areac[m];
    }

    let a1 = dpara(&p0, &p1, &p2);
    let a2 = dpara(&p0, &p1, &p3);
    let a3 = dpara(&p0, &p2, &p3);
    let a4 = a1 + a3 - a2;

    if a2 == a1 {
        return None;
    }

    let t = a3 / (a3 - a4);
    let s = a2 / (a2 - a1);
    let big_a = a2 * t / 2.0;

    if big_a == 0.0 {
        return None;
    }

    let r = area / big_a;
    let alpha = 2.0 - (4.0 - r / 0.3).sqrt();

    let c0 = interval(t * alpha, &p0, &p1);
    let c1 = interval(s * alpha, &p3, &p2);

    let mut res = OptiResult {
        pen: 0.0,
        c: [c0, c1],
        t,
        s,
        alpha,
    };

    let cp0 = res.c[0];
    let cp1 = res.c[1];

    // Check tangency against vertex edges
    k = modulo((i + 1) as i32, m as i32);
    while k != j {
        let kk1 = modulo((k + 1) as i32, m as i32);
        let t_val = tangent(
            &p0,
            &cp0,
            &cp1,
            &p3,
            &curve.segments[k].vertex,
            &curve.segments[kk1].vertex,
        );
        if t_val < -0.5 {
            return None;
        }
        let pt = bezier(t_val, &p0, &cp0, &cp1, &p3);
        let d = ddist(&curve.segments[k].vertex, &curve.segments[kk1].vertex);
        if d == 0.0 {
            return None;
        }
        let d1 = dpara(&curve.segments[k].vertex, &curve.segments[kk1].vertex, &pt) / d;
        if d1.abs() > opttolerance {
            return None;
        }
        if iprod(&curve.segments[k].vertex, &curve.segments[kk1].vertex, &pt) < 0.0
            || iprod(&curve.segments[kk1].vertex, &curve.segments[k].vertex, &pt) < 0.0
        {
            return None;
        }
        res.pen += sq(d1);
        k = kk1;
    }

    // Check tangency against endpoint edges
    k = i;
    while k != j {
        let kk1 = modulo((k + 1) as i32, m as i32);
        let t_val = tangent(
            &p0,
            &cp0,
            &cp1,
            &p3,
            &curve.segments[k].c[2],
            &curve.segments[kk1].c[2],
        );
        if t_val < -0.5 {
            return None;
        }
        let pt = bezier(t_val, &p0, &cp0, &cp1, &p3);
        let d = ddist(&curve.segments[k].c[2], &curve.segments[kk1].c[2]);
        if d == 0.0 {
            return None;
        }
        let d1 = dpara(&curve.segments[k].c[2], &curve.segments[kk1].c[2], &pt) / d;
        let d2 = dpara(
            &curve.segments[k].c[2],
            &curve.segments[kk1].c[2],
            &curve.segments[kk1].vertex,
        ) / d
            * 0.75
            * curve.segments[kk1].alpha;
        let (d1, d2) = if d2 < 0.0 { (-d1, -d2) } else { (d1, d2) };
        if d1 < d2 - opttolerance {
            return None;
        }
        if d1 < d2 {
            res.pen += sq(d1 - d2);
        }
        k = kk1;
    }

    Some(res)
}

/// Optimize curve by merging adjacent segments using dynamic programming.
pub fn opticurve(path: &mut InternalPath, opttolerance: f64) {
    let curve = path.curve.as_ref().unwrap();
    let m = curve.n();

    let mut pt: Vec<usize> = vec![0; m + 1];
    let mut pen: Vec<f64> = vec![0.0; m + 1];
    let mut length: Vec<usize> = vec![0; m + 1];
    let mut opt: Vec<Option<OptiResult>> = (0..=m).map(|_| None).collect();

    // Precompute convexity
    let mut convc: Vec<i32> = vec![0; m];
    for i in 0..m {
        if curve.segments[i].tag == SegmentTag::Curve {
            convc[i] = sign(dpara(
                &curve.segments[modulo(i as i32 - 1, m as i32)].vertex,
                &curve.segments[i].vertex,
                &curve.segments[modulo((i + 1) as i32, m as i32)].vertex,
            ));
        } else {
            convc[i] = 0;
        }
    }

    // Precompute area sums
    let mut areac: Vec<f64> = vec![0.0; m + 1];
    let mut area = 0.0;
    areac[0] = 0.0;
    let p0 = curve.segments[0].vertex;
    for i in 0..m {
        let i1 = modulo((i + 1) as i32, m as i32);
        if curve.segments[i1].tag == SegmentTag::Curve {
            let alpha = curve.segments[i1].alpha;
            area += 0.3
                * alpha
                * (4.0 - alpha)
                * dpara(
                    &curve.segments[i].c[2],
                    &curve.segments[i1].vertex,
                    &curve.segments[i1].c[2],
                )
                / 2.0;
            area += dpara(&p0, &curve.segments[i].c[2], &curve.segments[i1].c[2]) / 2.0;
        }
        areac[i + 1] = area;
    }

    // DP
    pt[0] = usize::MAX; // sentinel (-1 as usize)
    pen[0] = 0.0;
    length[0] = 0;

    for j in 1..=m {
        pt[j] = j - 1;
        pen[j] = pen[j - 1];
        length[j] = length[j - 1] + 1;

        for i in (0..j.saturating_sub(1)).rev() {
            let o = opti_penalty(path, i, j % m, opttolerance, &convc, &areac);
            match o {
                None => break,
                Some(o) => {
                    if length[j] > length[i] + 1
                        || (length[j] == length[i] + 1 && pen[j] > pen[i] + o.pen)
                    {
                        pt[j] = i;
                        pen[j] = pen[i] + o.pen;
                        length[j] = length[i] + 1;
                        opt[j] = Some(o);
                    }
                }
            }
        }
    }

    let om = length[m];

    // Reconstruct optimal curve
    let mut ocurve = InternalCurve::new(om);
    let mut s_arr: Vec<f64> = vec![0.0; om];
    let mut t_arr: Vec<f64> = vec![0.0; om];

    let curve = path.curve.as_ref().unwrap();

    // Walk backwards to get the optimal segment boundaries
    let mut j = m;
    let mut seg_idx = om;
    while seg_idx > 0 {
        seg_idx -= 1;
        let i = pt[j];
        let jm = j % m;

        if let Some(ref o) = opt[j] {
            // Optimized merged segment
            ocurve.segments[seg_idx].tag = SegmentTag::Curve;
            ocurve.segments[seg_idx].c[0] = o.c[0];
            ocurve.segments[seg_idx].c[1] = o.c[1];
            ocurve.segments[seg_idx].c[2] = curve.segments[jm].c[2];
            ocurve.segments[seg_idx].vertex = curve.segments[jm].vertex;
            ocurve.segments[seg_idx].alpha = o.alpha;
            s_arr[seg_idx] = o.s;
            t_arr[seg_idx] = o.t;
        } else {
            // Copy original segment
            let im1 = modulo((i + 1) as i32, m as i32);
            ocurve.segments[seg_idx].tag = curve.segments[im1].tag;
            ocurve.segments[seg_idx].c[0] = curve.segments[im1].c[0];
            ocurve.segments[seg_idx].c[1] = curve.segments[im1].c[1];
            ocurve.segments[seg_idx].c[2] = curve.segments[im1].c[2];
            ocurve.segments[seg_idx].vertex = curve.segments[im1].vertex;
            ocurve.segments[seg_idx].alpha = curve.segments[im1].alpha;
            s_arr[seg_idx] = 1.0;
            t_arr[seg_idx] = 1.0;
        }

        j = i;
    }

    // Compute beta values
    for i in 0..om {
        let i1 = modulo((i + 1) as i32, om as i32);
        let denom = s_arr[i] + t_arr[i1];
        ocurve.segments[i].beta = if denom != 0.0 { s_arr[i] / denom } else { 0.5 };
    }
    ocurve.alphacurve = true;

    path.ocurve = Some(ocurve);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::potrace::bitmap::*;
    use crate::potrace::polygon::*;
    use crate::potrace::types::*;

    fn traced_square_path() -> InternalPath {
        let mut bm = vec![vec![false; 12]; 12];
        for y in 3..9 {
            for x in 3..9 {
                bm[y][x] = true;
            }
        }
        let mut paths = bm_to_pathlist(&mut bm, 1, TurnPolicy::Minority);
        let mut path = paths.remove(0);
        calc_sums(&mut path);
        calc_lon(&mut path);
        bestpolygon(&mut path);
        adjust_vertices(&mut path);
        path
    }

    #[test]
    fn smooth_produces_segments_with_tags() {
        let mut path = traced_square_path();
        let curve = path.curve.as_mut().unwrap();
        smooth(curve, 1.0);
        assert!(curve.alphacurve);
        for seg in &curve.segments {
            assert!(seg.tag == SegmentTag::Corner || seg.tag == SegmentTag::Curve);
        }
    }

    #[test]
    fn smooth_with_zero_alphamax_produces_all_corners() {
        let mut path = traced_square_path();
        let curve = path.curve.as_mut().unwrap();
        smooth(curve, 0.0);
        for seg in &curve.segments {
            assert_eq!(seg.tag, SegmentTag::Corner);
        }
    }

    #[test]
    fn opticurve_produces_result() {
        let mut path = traced_square_path();
        if !path.sign {
            reverse(path.curve.as_mut().unwrap());
        }
        smooth(path.curve.as_mut().unwrap(), 1.0);
        opticurve(&mut path, 0.2);
        assert!(path.ocurve.is_some());
        let after = path.ocurve.as_ref().unwrap().n();
        let before = path.curve.as_ref().unwrap().n();
        assert!(
            after <= before,
            "optimization should not increase segment count"
        );
        assert!(after > 0);
    }
}
