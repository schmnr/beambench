// SPDX-FileCopyrightText: 2001-2019 Peter Selinger
// SPDX-FileCopyrightText: 2026 Beam Bench contributors
// SPDX-License-Identifier: GPL-2.0-or-later
//
// This file is a Rust port and modification of Potrace 1.16.
// Modified for Beam Bench on 2026-04-16: translated to Rust and adapted to
// Beam Bench's internal bitmap, path, and numeric representations.

#![allow(dead_code)]

use super::math_utils::detrand;
use super::types::{InternalPath, Point, TurnPolicy};

/// Python-style floor division (rounds toward negative infinity).
fn py_floordiv(a: i32, b: i32) -> i32 {
    if (a ^ b) < 0 && a % b != 0 {
        a / b - 1
    } else {
        a / b
    }
}

/// Safe bitmap read — returns false for out-of-bounds coordinates.
fn bm_get(bm: &[Vec<bool>], y: i32, x: i32) -> bool {
    if y < 0 || x < 0 {
        return false;
    }
    let yu = y as usize;
    let xu = x as usize;
    if yu < bm.len() && xu < bm[yu].len() {
        bm[yu][xu]
    } else {
        false
    }
}

/// Count set pixels in expanding diamond around intersection (x,y) to determine majority color.
pub fn majority(bm: &[Vec<bool>], x: i32, y: i32) -> bool {
    for i in 2..5 {
        let mut ct: i32 = 0;
        for a in (-i + 1)..=(i - 2) {
            ct += if bm_get(bm, y + i - 1, x + a) { 1 } else { -1 };
            ct += if bm_get(bm, y + a - 1, x + i - 1) {
                1
            } else {
                -1
            };
            ct += if bm_get(bm, y - i, x + a - 1) { 1 } else { -1 };
            ct += if bm_get(bm, y + a, x - i) { 1 } else { -1 };
        }
        if ct > 0 {
            return true;
        } else if ct < 0 {
            return false;
        }
    }
    false
}

/// XOR a horizontal line segment in row y between columns x and xa.
pub fn xor_to_ref(bm: &mut [Vec<bool>], x: usize, y: usize, xa: usize) {
    let lo = x.min(xa);
    let hi = x.max(xa);
    if lo == hi {
        return;
    }
    if y < bm.len() {
        for col in lo..hi {
            if col < bm[y].len() {
                bm[y][col] ^= true;
            }
        }
    }
}

/// XOR the interior of a traced path by scanning its edges.
pub fn xor_path(bm: &mut [Vec<bool>], path: &InternalPath) {
    if path.pt.is_empty() {
        return;
    }
    let mut y1 = path.pt.last().unwrap().y as i32;
    let xa = path.pt[0].x as usize;
    for n in &path.pt {
        let x = n.x as i32;
        let y = n.y as i32;
        if y != y1 {
            let min_y = y.min(y1);
            if min_y >= 0 {
                xor_to_ref(bm, x as usize, min_y as usize, xa);
            }
            y1 = y;
        }
    }
}

/// Determine whether to turn right at an ambiguous pixel boundary.
fn should_turn_right(turnpolicy: TurnPolicy, sign: bool, x: i32, y: i32, bm: &[Vec<bool>]) -> bool {
    match turnpolicy {
        TurnPolicy::Right => true,
        TurnPolicy::Left => false,
        TurnPolicy::Black => sign,
        TurnPolicy::White => !sign,
        TurnPolicy::Random => detrand(x, y),
        TurnPolicy::Majority => majority(bm, x, y),
        TurnPolicy::Minority => !majority(bm, x, y),
    }
}

/// Core boundary tracer. Walks pixel edges starting at (x0, y0), turning
/// left/right based on the turnpolicy to trace a closed contour.
pub fn findpath(
    bm: &[Vec<bool>],
    x0: i32,
    y0: i32,
    path_sign: bool,
    turnpolicy: TurnPolicy,
) -> InternalPath {
    let mut x = x0;
    let mut y = y0;
    let mut dirx: i32 = 0;
    let mut diry: i32 = -1;
    let mut pt = Vec::new();
    let mut area: i64 = 0;

    loop {
        pt.push(Point::new(x as f64, y as f64));
        x += dirx;
        y += diry;
        area += x as i64 * diry as i64;

        if x == x0 && y == y0 {
            break;
        }

        // Pixel to the left of direction
        let cy = y + py_floordiv(diry - dirx - 1, 2);
        let cx = x + py_floordiv(dirx + diry - 1, 2);
        let c = bm_get(bm, cy, cx);

        // Pixel to the right of direction
        let dy_coord = y + py_floordiv(diry + dirx - 1, 2);
        let dx_coord = x + py_floordiv(dirx - diry - 1, 2);
        let d = bm_get(bm, dy_coord, dx_coord);

        if c && !d {
            // Ambiguous — use turnpolicy
            if should_turn_right(turnpolicy, path_sign, x, y, bm) {
                // turn right
                let new_dirx = diry;
                let new_diry = -dirx;
                dirx = new_dirx;
                diry = new_diry;
            } else {
                // turn left
                let new_dirx = -diry;
                let new_diry = dirx;
                dirx = new_dirx;
                diry = new_diry;
            }
        } else if c {
            // c set, d set — turn right
            let new_dirx = diry;
            let new_diry = -dirx;
            dirx = new_dirx;
            diry = new_diry;
        } else if !d {
            // c clear, d clear — turn left
            let new_dirx = -diry;
            let new_diry = dirx;
            dirx = new_dirx;
            diry = new_diry;
        }
        // else: c clear, d set — go straight
    }

    InternalPath::new(pt, area, path_sign)
}

/// Find next set pixel, scanning top-to-bottom, left-to-right.
pub fn findnext(bm: &[Vec<bool>]) -> Option<(usize, usize)> {
    for y in 0..bm.len() {
        for x in 0..bm[y].len() {
            if bm[y][x] {
                return Some((y, x));
            }
        }
    }
    None
}

/// Main decomposition loop: trace all boundary contours from the bitmap.
/// Mutates `bm` by XOR-ing traced path interiors. Returns paths whose
/// absolute area exceeds `turdsize`.
pub fn bm_to_pathlist(
    bm: &mut Vec<Vec<bool>>,
    turdsize: i64,
    turnpolicy: TurnPolicy,
) -> Vec<InternalPath> {
    let original = bm.clone();
    let mut plist = Vec::new();

    loop {
        let n = findnext(bm);
        let Some((y, x)) = n else { break };

        let sign = original[y][x];
        let path = findpath(bm, x as i32, (y + 1) as i32, sign, turnpolicy);
        xor_path(bm, &path);

        if path.area.unsigned_abs() > turdsize as u64 {
            plist.push(path);
        }
    }

    plist
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bitmap(w: usize, h: usize, pixels: &[(usize, usize)]) -> Vec<Vec<bool>> {
        let mut bm = vec![vec![false; w + 1]; h + 1];
        for &(x, y) in pixels {
            if y < h + 1 && x < w + 1 {
                bm[y][x] = true;
            }
        }
        bm
    }

    #[test]
    fn findpath_traces_single_pixel() {
        let bm = make_bitmap(3, 3, &[(1, 1)]);
        let path = findpath(&bm, 1, 2, true, TurnPolicy::Minority);
        assert!(
            path.pt.len() >= 4,
            "single pixel should produce >= 4 boundary points"
        );
        assert!(path.area != 0);
    }

    #[test]
    fn findpath_traces_square() {
        let mut pixels = Vec::new();
        for y in 2..6 {
            for x in 2..6 {
                pixels.push((x, y));
            }
        }
        let bm = make_bitmap(8, 8, &pixels);
        let path = findpath(&bm, 2, 3, true, TurnPolicy::Minority);
        assert!(path.pt.len() >= 4);
    }

    #[test]
    fn bm_to_pathlist_finds_contours() {
        let mut pixels = Vec::new();
        for y in 2..6 {
            for x in 2..6 {
                pixels.push((x, y));
            }
        }
        let mut bm = make_bitmap(8, 8, &pixels);
        let paths = bm_to_pathlist(&mut bm, 2, TurnPolicy::Minority);
        assert!(!paths.is_empty(), "should find at least one contour");
    }

    #[test]
    fn bm_to_pathlist_filters_by_turdsize() {
        let mut bm = make_bitmap(5, 5, &[(2, 2)]);
        let paths = bm_to_pathlist(&mut bm, 2, TurnPolicy::Minority);
        assert!(
            paths.is_empty(),
            "single pixel should be filtered by turdsize=2"
        );
    }

    #[test]
    fn xor_path_inverts_interior() {
        let mut pixels = Vec::new();
        for y in 1..4 {
            for x in 1..4 {
                pixels.push((x, y));
            }
        }
        let mut bm = make_bitmap(5, 5, &pixels);
        assert!(bm[2][2]);
        let path = findpath(&bm, 1, 2, true, TurnPolicy::Minority);
        xor_path(&mut bm, &path);
        assert!(!bm[2][2], "center should be cleared after xor_path");
    }

    #[test]
    fn nested_contours_detected() {
        let mut pixels = Vec::new();
        for y in 1..9 {
            for x in 1..9 {
                if !(x >= 3 && x < 7 && y >= 3 && y < 7) {
                    pixels.push((x, y));
                }
            }
        }
        let mut bm = make_bitmap(10, 10, &pixels);
        let paths = bm_to_pathlist(&mut bm, 1, TurnPolicy::Minority);
        assert!(
            paths.len() >= 2,
            "ring shape should produce outer + inner contour, got {}",
            paths.len()
        );
    }

    #[test]
    fn py_floordiv_matches_python() {
        assert_eq!(py_floordiv(-3, 2), -2); // Python: -3 // 2 == -2
        assert_eq!(py_floordiv(-1, 2), -1); // Python: -1 // 2 == -1
        assert_eq!(py_floordiv(1, 2), 0); // Python: 1 // 2 == 0
        assert_eq!(py_floordiv(3, 2), 1); // Python: 3 // 2 == 1
        assert_eq!(py_floordiv(-4, 2), -2); // Python: -4 // 2 == -2
    }
}
