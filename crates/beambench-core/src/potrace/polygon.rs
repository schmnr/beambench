// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

#![allow(dead_code)]

use super::math_utils::*;
use super::types::*;

const INFTY: i64 = 10_000_000;

fn sq(x: f64) -> f64 {
    x * x
}

/// Compute cumulative sums for O(1) penalty computation.
pub fn calc_sums(path: &mut InternalPath) {
    let n = path.len();
    path.sums = vec![Sums::default(); n + 1];
    path.x0 = path.pt[0].x;
    path.y0 = path.pt[0].y;

    for i in 0..n {
        let x = path.pt[i].x - path.x0;
        let y = path.pt[i].y - path.y0;
        path.sums[i + 1].x = path.sums[i].x + x;
        path.sums[i + 1].y = path.sums[i].y + y;
        path.sums[i + 1].x2 = path.sums[i].x2 + x * x;
        path.sums[i + 1].xy = path.sums[i].xy + x * y;
        path.sums[i + 1].y2 = path.sums[i].y2 + y * y;
    }
}

/// Compute longest straight subpaths.
pub fn calc_lon(path: &mut InternalPath) {
    let n = path.len();
    let pt = &path.pt;
    let mut ct = [0i32; 4];
    let mut pivk = vec![0usize; n];
    let mut nc = vec![0usize; n];

    // Compute nc: next corner
    let mut k = 0usize;
    for i in (0..n).rev() {
        if pt[i].x != pt[k].x && pt[i].y != pt[k].y {
            k = i + 1;
        }
        nc[i] = k;
    }

    path.lon = vec![0usize; n];

    for i in (0..n).rev() {
        ct[0] = 0;
        ct[1] = 0;
        ct[2] = 0;
        ct[3] = 0;

        let i1 = modulo(i as i32 + 1, n as i32);
        let dir = (3 + 3 * (pt[i1].x - pt[i].x) as i32 + (pt[i1].y - pt[i].y) as i32) / 2;
        ct[dir as usize] += 1;

        let mut constraint0x: f64 = 0.0;
        let mut constraint0y: f64 = 0.0;
        let mut constraint1x: f64 = 0.0;
        let mut constraint1y: f64 = 0.0;

        k = nc[i];
        let mut k1 = i;
        let mut break_inner = false;

        loop {
            let dir = (3 + 3 * sign(pt[k].x - pt[k1].x) + sign(pt[k].y - pt[k1].y)) / 2;
            ct[dir as usize] += 1;

            if ct[0] != 0 && ct[1] != 0 && ct[2] != 0 && ct[3] != 0 {
                pivk[i] = k1;
                break_inner = true;
                break;
            }

            let cur_x = pt[k].x - pt[i].x;
            let cur_y = pt[k].y - pt[i].y;

            if xprod(constraint0x, constraint0y, cur_x, cur_y) < 0.0
                || xprod(constraint1x, constraint1y, cur_x, cur_y) > 0.0
            {
                break;
            }

            if cur_x.abs() <= 1.0 && cur_y.abs() <= 1.0 {
                // pass
            } else {
                let off_x = cur_x
                    + if cur_y >= 0.0 && (cur_y > 0.0 || cur_x < 0.0) {
                        1.0
                    } else {
                        -1.0
                    };
                let off_y = cur_y
                    + if cur_x <= 0.0 && (cur_x < 0.0 || cur_y < 0.0) {
                        1.0
                    } else {
                        -1.0
                    };
                if xprod(constraint0x, constraint0y, off_x, off_y) >= 0.0 {
                    constraint0x = off_x;
                    constraint0y = off_y;
                }

                let off_x = cur_x
                    + if cur_y <= 0.0 && (cur_y < 0.0 || cur_x < 0.0) {
                        1.0
                    } else {
                        -1.0
                    };
                let off_y = cur_y
                    + if cur_x >= 0.0 && (cur_x > 0.0 || cur_y < 0.0) {
                        1.0
                    } else {
                        -1.0
                    };
                if xprod(constraint1x, constraint1y, off_x, off_y) <= 0.0 {
                    constraint1x = off_x;
                    constraint1y = off_y;
                }
            }

            k1 = k;
            k = nc[k1];
            if !cyclic(k, i, k1) {
                break;
            }
        }

        if break_inner {
            continue;
        }

        // Compute pivot from constraints
        let dk_x = sign(pt[k].x - pt[k1].x) as f64;
        let dk_y = sign(pt[k].y - pt[k1].y) as f64;
        let cur_x = pt[k1].x - pt[i].x;
        let cur_y = pt[k1].y - pt[i].y;

        let a = xprod(constraint0x, constraint0y, cur_x, cur_y) as i64;
        let b = xprod(constraint0x, constraint0y, dk_x, dk_y) as i64;
        let c = xprod(constraint1x, constraint1y, cur_x, cur_y) as i64;
        let d = xprod(constraint1x, constraint1y, dk_x, dk_y) as i64;

        let mut j = INFTY;
        if b < 0 {
            j = floordiv(a, -b);
        }
        if d > 0 {
            j = j.min(floordiv(-c, d));
        }
        pivk[i] = modulo(k1 as i32 + j as i32, n as i32);
    }

    // Compute lon from pivk
    let mut j = pivk[n - 1];
    path.lon[n - 1] = j;
    for i in (0..n - 1).rev() {
        if cyclic(i + 1, pivk[i], j) {
            j = pivk[i];
        }
        path.lon[i] = j;
    }

    let mut i = n - 1;
    loop {
        let i1 = modulo(i as i32 + 1, n as i32);
        if !cyclic(i1, j, path.lon[i]) {
            break;
        }
        path.lon[i] = j;
        if i == 0 {
            break;
        }
        i -= 1;
    }
}

/// Penalty for approximating path[i..j] with a single edge.
pub fn penalty3(path: &InternalPath, i: usize, j: usize) -> f64 {
    let n = path.len();
    let pt = &path.pt;
    let sums = &path.sums;

    let mut j = j;
    let mut r = 0;
    if j >= n {
        j -= n;
        r = 1;
    }

    let (x, y, x2, xy, y2, k);
    if r == 0 {
        x = sums[j + 1].x - sums[i].x;
        y = sums[j + 1].y - sums[i].y;
        x2 = sums[j + 1].x2 - sums[i].x2;
        xy = sums[j + 1].xy - sums[i].xy;
        y2 = sums[j + 1].y2 - sums[i].y2;
        k = (j + 1 - i) as f64;
    } else {
        x = sums[j + 1].x - sums[i].x + sums[n].x;
        y = sums[j + 1].y - sums[i].y + sums[n].y;
        x2 = sums[j + 1].x2 - sums[i].x2 + sums[n].x2;
        xy = sums[j + 1].xy - sums[i].xy + sums[n].xy;
        y2 = sums[j + 1].y2 - sums[i].y2 + sums[n].y2;
        k = (j + 1 + n - i) as f64;
    }

    let px = (pt[i].x + pt[j].x) / 2.0 - pt[0].x;
    let py = (pt[i].y + pt[j].y) / 2.0 - pt[0].y;
    let ey = pt[j].x - pt[i].x;
    let ex = -(pt[j].y - pt[i].y);

    let a = (x2 - 2.0 * x * px) / k + px * px;
    let b = (xy - x * py - y * px) / k + px * py;
    let c = (y2 - 2.0 * y * py) / k + py * py;

    let s = ex * ex * a + 2.0 * ex * ey * b + ey * ey * c;
    s.sqrt()
}

/// Find optimal polygon approximation via dynamic programming.
pub fn bestpolygon(path: &mut InternalPath) {
    let n = path.len();
    let mut pen = vec![0.0f64; n + 1];
    let mut prev = vec![0usize; n + 1];
    let mut clip0 = vec![0usize; n];
    let mut clip1 = vec![0usize; n + 1];
    let mut seg0 = vec![0usize; n + 1];
    let mut seg1 = vec![0usize; n + 1];

    for i in 0..n {
        let c = modulo(
            path.lon[modulo(i as i32 - 1, n as i32)] as i32 - 1,
            n as i32,
        );
        if c == i {
            clip0[i] = modulo(i as i32 + 1, n as i32);
        } else if c < i {
            clip0[i] = n;
        } else {
            clip0[i] = c;
        }
    }

    let mut j = 1usize;
    for i in 0..n {
        while j <= clip0[i] {
            clip1[j] = i;
            j += 1;
        }
    }

    let mut i = 0usize;
    j = 0;
    while i < n {
        seg0[j] = i;
        i = clip0[i];
        j += 1;
    }
    seg0[j] = n;
    let m = j;

    i = n;
    for jj in (1..=m).rev() {
        seg1[jj] = i;
        i = clip1[i];
    }
    seg1[0] = 0;

    pen[0] = 0.0;
    for jj in 1..=m {
        for ii in seg1[jj]..=seg0[jj] {
            let mut best = -1.0f64;
            let mut kk = seg0[jj - 1] as i64;
            while kk >= clip1[ii] as i64 {
                let thispen = penalty3(path, kk as usize, ii) + pen[kk as usize];
                if best < 0.0 || thispen < best {
                    prev[ii] = kk as usize;
                    best = thispen;
                }
                kk -= 1;
            }
            pen[ii] = best;
        }
    }

    path.m = m;
    path.po = vec![0usize; m];

    i = n;
    j = m - 1;
    loop {
        i = prev[i];
        path.po[j] = i;
        if j == 0 {
            break;
        }
        j -= 1;
    }
}

/// Fit a line through points i..j using PCA. Returns (center, direction).
fn pointslope(path: &InternalPath, i: i32, j: i32) -> (Point, Point) {
    let n = path.len() as i32;
    let sums = &path.sums;

    let mut jj = j;
    let mut r = 0i32;
    while jj >= n {
        jj -= n;
        r += 1;
    }
    while jj < 0 {
        jj += n;
        r -= 1;
    }

    let mut ii = i;
    while ii >= n {
        ii -= n;
        r -= 1;
    }
    while ii < 0 {
        ii += n;
        r += 1;
    }

    let ii = ii as usize;
    let jj = jj as usize;
    let nn = n as usize;

    let x = sums[jj + 1].x - sums[ii].x + r as f64 * sums[nn].x;
    let y = sums[jj + 1].y - sums[ii].y + r as f64 * sums[nn].y;
    let x2 = sums[jj + 1].x2 - sums[ii].x2 + r as f64 * sums[nn].x2;
    let xy = sums[jj + 1].xy - sums[ii].xy + r as f64 * sums[nn].xy;
    let y2 = sums[jj + 1].y2 - sums[ii].y2 + r as f64 * sums[nn].y2;
    let k = (jj as i32 + 1 - ii as i32 + r * n) as f64;

    let ctr = Point::new(x / k, y / k);

    let a = (x2 - x * x / k) / k;
    let b = (xy - x * y / k) / k;
    let c = (y2 - y * y / k) / k;

    let lambda2 = (a + c + ((a - c) * (a - c) + 4.0 * b * b).sqrt()) / 2.0;

    let a = a - lambda2;
    let c = c - lambda2;

    let dir;
    if a.abs() >= c.abs() {
        let l = (a * a + b * b).sqrt();
        if l != 0.0 {
            dir = Point::new(-b / l, a / l);
        } else {
            dir = Point::new(0.0, 0.0);
        }
    } else {
        let l = (c * c + b * b).sqrt();
        if l != 0.0 {
            dir = Point::new(-c / l, b / l);
        } else {
            dir = Point::new(0.0, 0.0);
        }
    }

    (ctr, dir)
}

/// Adjust polygon vertices to minimize error using quadratic form optimization.
pub fn adjust_vertices(path: &mut InternalPath) {
    let m = path.m;
    let n = path.len();
    let x0 = path.x0;
    let y0 = path.y0;

    let mut ctr = vec![Point::default(); m];
    let mut dir = vec![Point::default(); m];
    let mut q: Vec<[[f64; 3]; 3]> = vec![[[0.0; 3]; 3]; m];

    path.curve = Some(InternalCurve::new(m));

    // Compute pointslope for each polygon edge
    for i in 0..m {
        let j_idx = path.po[modulo(i as i32 + 1, m as i32)];
        let j_adjusted = modulo(j_idx as i32 - path.po[i] as i32, n as i32) + path.po[i];
        let (c, d) = pointslope(path, path.po[i] as i32, j_adjusted as i32);
        ctr[i] = c;
        dir[i] = d;
    }

    // Build quadratic form for each edge
    for i in 0..m {
        let d = sq(dir[i].x) + sq(dir[i].y);
        if d == 0.0 {
            q[i] = [[0.0; 3]; 3];
        } else {
            let v0 = dir[i].y;
            let v1 = -dir[i].x;
            let v2 = -v1 * ctr[i].y - v0 * ctr[i].x;
            let v = [v0, v1, v2];
            for l in 0..3 {
                for kk in 0..3 {
                    q[i][l][kk] = v[l] * v[kk] / d;
                }
            }
        }
    }

    // Optimize each vertex
    let mut big_q = [[0.0f64; 3]; 3];
    for i in 0..m {
        let poi = path.po[i];
        let s_pt = Point::new(path.pt[poi].x - x0, path.pt[poi].y - y0);
        let j = modulo(i as i32 - 1, m as i32);

        for l in 0..3 {
            for kk in 0..3 {
                big_q[l][kk] = q[j][l][kk] + q[i][l][kk];
            }
        }

        // Try to solve the 2x2 system
        let mut w;

        loop {
            let det = big_q[0][0] * big_q[1][1] - big_q[0][1] * big_q[1][0];
            if det != 0.0 {
                w = Point::new(
                    (-big_q[0][2] * big_q[1][1] + big_q[1][2] * big_q[0][1]) / det,
                    (big_q[0][2] * big_q[1][0] - big_q[1][2] * big_q[0][0]) / det,
                );
                break;
            }

            // Singular — inject constraint
            let v;
            if big_q[0][0] > big_q[1][1] {
                v = [-big_q[0][1], big_q[0][0], 0.0];
            } else if big_q[1][1] != 0.0 {
                v = [-big_q[1][1], big_q[1][0], 0.0];
            } else {
                v = [1.0, 0.0, 0.0];
            }

            let d = sq(v[0]) + sq(v[1]);
            let v2 = -v[1] * s_pt.y - v[0] * s_pt.x;
            let vv = [v[0], v[1], v2];
            for l in 0..3 {
                for kk in 0..3 {
                    big_q[l][kk] += vv[l] * vv[kk] / d;
                }
            }
        }

        let dx = (w.x - s_pt.x).abs();
        let dy = (w.y - s_pt.y).abs();
        if dx <= 0.5 && dy <= 0.5 {
            let curve = path.curve.as_mut().unwrap();
            curve.segments[i].vertex.x = w.x + x0;
            curve.segments[i].vertex.y = w.y + y0;
            continue;
        }

        // Fallback: search along constraint boundaries
        let mut min_val = quadform(&big_q, &s_pt);
        let mut xmin = s_pt.x;
        let mut ymin = s_pt.y;

        if big_q[0][0] != 0.0 {
            for z in 0..2 {
                w.y = s_pt.y - 0.5 + z as f64;
                w.x = -(big_q[0][1] * w.y + big_q[0][2]) / big_q[0][0];
                let dx = (w.x - s_pt.x).abs();
                let cand = quadform(&big_q, &w);
                if dx <= 0.5 && cand < min_val {
                    min_val = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }

        if big_q[1][1] != 0.0 {
            for z in 0..2 {
                w.x = s_pt.x - 0.5 + z as f64;
                w.y = -(big_q[1][0] * w.x + big_q[1][2]) / big_q[1][1];
                let dy = (w.y - s_pt.y).abs();
                let cand = quadform(&big_q, &w);
                if dy <= 0.5 && cand < min_val {
                    min_val = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }

        for l in 0..2 {
            for kk in 0..2 {
                w = Point::new(s_pt.x - 0.5 + l as f64, s_pt.y - 0.5 + kk as f64);
                let cand = quadform(&big_q, &w);
                if cand < min_val {
                    min_val = cand;
                    xmin = w.x;
                    ymin = w.y;
                }
            }
        }

        let curve = path.curve.as_mut().unwrap();
        curve.segments[i].vertex.x = xmin + x0;
        curve.segments[i].vertex.y = ymin + y0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::potrace::types::{InternalPath, Point};

    fn square_path() -> InternalPath {
        let pts = vec![
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(2.0, 0.0),
            Point::new(3.0, 0.0),
            Point::new(4.0, 0.0),
            Point::new(4.0, 1.0),
            Point::new(4.0, 2.0),
            Point::new(4.0, 3.0),
            Point::new(4.0, 4.0),
            Point::new(3.0, 4.0),
            Point::new(2.0, 4.0),
            Point::new(1.0, 4.0),
            Point::new(0.0, 4.0),
            Point::new(0.0, 3.0),
            Point::new(0.0, 2.0),
            Point::new(0.0, 1.0),
        ];
        InternalPath::new(pts, 16, true)
    }

    #[test]
    fn calc_sums_populates_cumulative_arrays() {
        let mut path = square_path();
        calc_sums(&mut path);
        assert_eq!(path.sums.len(), path.len() + 1);
        assert_eq!(path.sums[0].x, 0.0);
        assert_eq!(path.sums[0].y, 0.0);
    }

    #[test]
    fn calc_lon_finds_straight_segments() {
        let mut path = square_path();
        calc_sums(&mut path);
        calc_lon(&mut path);
        for i in 0..path.len() {
            assert!(path.lon[i] > 0 || i == path.len() - 1);
        }
    }

    #[test]
    fn bestpolygon_finds_optimal_polygon() {
        let mut path = square_path();
        calc_sums(&mut path);
        calc_lon(&mut path);
        bestpolygon(&mut path);
        assert!(
            path.m >= 4,
            "square should have >= 4 polygon segments, got {}",
            path.m
        );
    }

    #[test]
    fn adjust_vertices_produces_curve() {
        let mut path = square_path();
        calc_sums(&mut path);
        calc_lon(&mut path);
        bestpolygon(&mut path);
        adjust_vertices(&mut path);
        assert!(path.curve.is_some());
        let curve = path.curve.as_ref().unwrap();
        assert_eq!(curve.n(), path.m);
    }

    #[test]
    fn full_pipeline_on_traced_bitmap() {
        // Trace a real bitmap to get realistic path data
        use crate::potrace::bitmap::bm_to_pathlist;
        use crate::potrace::types::TurnPolicy;
        let mut bm = vec![vec![false; 12]; 12];
        for y in 3..9 {
            for x in 3..9 {
                bm[y][x] = true;
            }
        }
        let mut paths = bm_to_pathlist(&mut bm, 1, TurnPolicy::Minority);
        assert!(!paths.is_empty());
        let path = &mut paths[0];
        calc_sums(path);
        calc_lon(path);
        bestpolygon(path);
        adjust_vertices(path);
        assert!(path.curve.is_some());
        assert!(path.m >= 4);
    }
}
