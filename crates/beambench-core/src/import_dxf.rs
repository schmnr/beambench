//! DXF file import - text parser for common entities (LINE, CIRCLE, ARC, LWPOLYLINE).

use beambench_common::path::{PathCommand, SubPath, VecPath};
use serde::{Deserialize, Serialize};

/// A single entity extracted from a DXF file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DxfEntity {
    pub layer_name: String,
    pub path: VecPath,
}

/// Parse DXF text content and extract vector paths.
/// Supports LINE, POLYLINE, LWPOLYLINE, CIRCLE, ARC entities.
pub fn parse_dxf(content: &str) -> Result<Vec<DxfEntity>, String> {
    let pairs = parse_dxf_pairs(content);
    let scale = dxf_insunits_scale_to_mm(&pairs);
    let mut entities = Vec::new();
    let mut i = 0;

    while i < pairs.len() {
        let (code, value) = &pairs[i];

        // Look for entity section
        if *code == 0 && value == "SECTION" {
            i += 1;
            if i < pairs.len() && pairs[i].0 == 2 && pairs[i].1 == "ENTITIES" {
                i += 1;
                // Parse entities until ENDSEC
                while i < pairs.len() {
                    if pairs[i].0 == 0 && pairs[i].1 == "ENDSEC" {
                        break;
                    }
                    if pairs[i].0 == 0 {
                        if let Some((entity, consumed)) = parse_entity(&pairs[i..]) {
                            entities.push(entity);
                            i += consumed;
                        } else {
                            i += 1;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
        }
        i += 1;
    }

    // Scale all coordinates to millimeters per the file's declared units.
    if scale != 1.0 {
        for entity in &mut entities {
            for subpath in &mut entity.path.subpaths {
                for command in &mut subpath.commands {
                    match command {
                        PathCommand::MoveTo { x, y } | PathCommand::LineTo { x, y } => {
                            *x *= scale;
                            *y *= scale;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(entities)
}

/// Millimeter scale factor for the file's `$INSUNITS` header variable.
/// DXF unit codes: 1=inches, 2=feet, 4=mm, 5=cm, 6=m. Anything else
/// (including 0 = unitless or a missing header) is treated as already mm.
fn dxf_insunits_scale_to_mm(pairs: &[(i32, String)]) -> f64 {
    for (i, pair) in pairs.iter().enumerate() {
        if pair.0 == 9
            && pair.1 == "$INSUNITS"
            && let Some(next) = pairs.get(i + 1)
            && next.0 == 70
        {
            return match next.1.parse::<i32>().unwrap_or(0) {
                1 => 25.4,
                2 => 304.8,
                5 => 10.0,
                6 => 1000.0,
                _ => 1.0,
            };
        }
    }
    1.0
}

/// Parse DXF group codes (integer code + value pairs separated by newlines).
fn parse_dxf_pairs(content: &str) -> Vec<(i32, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let mut pairs = Vec::new();
    let mut i = 0;

    while i + 1 < lines.len() {
        if let Ok(code) = lines[i].trim().parse::<i32>() {
            let value = lines[i + 1].trim().to_string();
            pairs.push((code, value));
            i += 2;
        } else {
            i += 1;
        }
    }

    pairs
}

/// Parse a single DXF entity from pairs, returning (entity, consumed_count).
fn parse_entity(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    if pairs.is_empty() || pairs[0].0 != 0 {
        return None;
    }

    let entity_type = &pairs[0].1;
    match entity_type.as_str() {
        "LINE" => parse_line(pairs),
        "CIRCLE" => parse_circle(pairs),
        "ARC" => parse_arc(pairs),
        "LWPOLYLINE" => parse_lwpolyline(pairs),
        _ => None,
    }
}

#[allow(clippy::needless_range_loop)]
fn parse_line(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    let mut x1 = 0.0;
    let mut y1 = 0.0;
    let mut x2 = 0.0;
    let mut y2 = 0.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break, // Next entity
            8 => layer = pairs[i].1.clone(),
            10 => x1 = pairs[i].1.parse().unwrap_or(0.0),
            20 => y1 = pairs[i].1.parse().unwrap_or(0.0),
            11 => x2 = pairs[i].1.parse().unwrap_or(0.0),
            21 => y2 = pairs[i].1.parse().unwrap_or(0.0),
            _ => {}
        }
        consumed = i + 1;
    }

    let mut sp = SubPath::new();
    sp.commands.push(PathCommand::MoveTo { x: x1, y: y1 });
    sp.commands.push(PathCommand::LineTo { x: x2, y: y2 });

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_circle(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut radius = 0.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            10 => cx = pairs[i].1.parse().unwrap_or(0.0),
            20 => cy = pairs[i].1.parse().unwrap_or(0.0),
            40 => radius = pairs[i].1.parse().unwrap_or(0.0),
            _ => {}
        }
        consumed = i + 1;
    }

    // Approximate circle with 32-sided polygon
    let segments = 32;
    let mut sp = SubPath::new();

    for i in 0..segments {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (segments as f64);
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();

        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x, y });
        } else {
            sp.commands.push(PathCommand::LineTo { x, y });
        }
    }
    sp.commands.push(PathCommand::Close);
    sp.closed = true;

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_arc(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    let mut cx = 0.0;
    let mut cy = 0.0;
    let mut radius = 0.0;
    let mut start_angle: f64 = 0.0;
    let mut end_angle: f64 = 360.0;
    let mut layer = "0".to_string();
    let mut consumed = 1;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            10 => cx = pairs[i].1.parse().unwrap_or(0.0),
            20 => cy = pairs[i].1.parse().unwrap_or(0.0),
            40 => radius = pairs[i].1.parse().unwrap_or(0.0),
            50 => start_angle = pairs[i].1.parse().unwrap_or(0.0),
            51 => end_angle = pairs[i].1.parse().unwrap_or(360.0),
            _ => {}
        }
        consumed = i + 1;
    }

    // Approximate arc with line segments
    let segments = 16;
    let start_rad = start_angle.to_radians();
    let end_rad = end_angle.to_radians();
    let delta = (end_rad - start_rad) / (segments as f64);

    let mut sp = SubPath::new();

    for i in 0..=segments {
        let angle = start_rad + delta * (i as f64);
        let x = cx + radius * angle.cos();
        let y = cy + radius * angle.sin();

        if i == 0 {
            sp.commands.push(PathCommand::MoveTo { x, y });
        } else {
            sp.commands.push(PathCommand::LineTo { x, y });
        }
    }

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[allow(clippy::needless_range_loop)]
fn parse_lwpolyline(pairs: &[(i32, String)]) -> Option<(DxfEntity, usize)> {
    let mut vertices: Vec<(f64, f64)> = Vec::new();
    let mut layer = "0".to_string();
    let mut consumed = 1;
    let mut current_x = None;
    let mut closed = false;

    for i in 1..pairs.len() {
        match pairs[i].0 {
            0 => break,
            8 => layer = pairs[i].1.clone(),
            // Polyline flag: bit 0 = closed
            70 => closed = (pairs[i].1.trim().parse::<i32>().unwrap_or(0) & 1) != 0,
            10 => current_x = Some(pairs[i].1.parse().unwrap_or(0.0)),
            20 => {
                if let Some(x) = current_x.take() {
                    let y = pairs[i].1.parse().unwrap_or(0.0);
                    vertices.push((x, y));
                }
            }
            _ => {}
        }
        consumed = i + 1;
    }

    if vertices.is_empty() {
        return None;
    }

    let mut sp = SubPath::new();
    for (idx, (x, y)) in vertices.iter().enumerate() {
        if idx == 0 {
            sp.commands.push(PathCommand::MoveTo { x: *x, y: *y });
        } else {
            sp.commands.push(PathCommand::LineTo { x: *x, y: *y });
        }
    }
    if closed {
        sp.commands.push(PathCommand::Close);
        sp.closed = true;
    }

    Some((
        DxfEntity {
            layer_name: layer,
            path: VecPath { subpaths: vec![sp] },
        },
        consumed,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_line() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\nLayer1\n10\n10.0\n20\n20.0\n11\n30.0\n21\n40.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].layer_name, "Layer1");
        assert_eq!(entities[0].path.subpaths.len(), 1);
        assert_eq!(entities[0].path.subpaths[0].commands.len(), 2);
    }

    #[test]
    fn parse_circle_creates_closed_path() {
        let dxf =
            "0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n50.0\n20\n50.0\n40\n25.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert!(entities[0].path.subpaths[0].closed);
        // Circle approximated with 32 segments + close
        assert_eq!(entities[0].path.subpaths[0].commands.len(), 33);
    }

    #[test]
    fn parse_arc_creates_open_path() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nARC\n8\n0\n10\n0.0\n20\n0.0\n40\n10.0\n50\n0.0\n51\n90.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert!(!entities[0].path.subpaths[0].closed);
        assert!(entities[0].path.subpaths[0].commands.len() > 1);
    }

    #[test]
    fn parse_lwpolyline() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n10\n20.0\n20\n0.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].path.subpaths[0].commands.len(), 3);
        assert!(
            !entities[0].path.subpaths[0].closed,
            "open polyline (no flag) must stay open"
        );
    }

    #[test]
    fn parse_lwpolyline_closed_flag() {
        // Group code 70 = 1 (bit 0 set) marks the polyline as closed.
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n90\n3\n70\n1\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n10\n20.0\n20\n0.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 1);
        let sp = &entities[0].path.subpaths[0];
        assert!(sp.closed, "code 70 bit 0 must close the subpath");
        assert_eq!(sp.commands.len(), 4); // M L L Z
        assert_eq!(sp.commands[3], PathCommand::Close);
    }

    #[test]
    fn parse_lwpolyline_plinegen_flag_not_closed() {
        // Code 70 = 128 (plinegen) has bit 0 clear — must not close.
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n8\n0\n70\n128\n10\n0.0\n20\n0.0\n10\n10.0\n20\n10.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert!(!entities[0].path.subpaths[0].closed);
    }

    #[test]
    fn parse_multiple_entities() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n10\n0.0\n20\n0.0\n11\n10.0\n21\n10.0\n0\nCIRCLE\n10\n5.0\n20\n5.0\n40\n2.0\n0\nENDSEC\n";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 2);
    }

    #[test]
    fn parse_empty_dxf() {
        let dxf = "";
        let entities = parse_dxf(dxf).unwrap();
        assert_eq!(entities.len(), 0);
    }

    #[test]
    fn dxf_insunits_inches_scales_to_mm() {
        // Header declares $INSUNITS = 1 (inches); a 1-unit line must become 25.4 mm.
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n1.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 25.4).abs() < 1e-6, "expected 25.4mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn dxf_insunits_absent_is_unscaled() {
        let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n10.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 10.0).abs() < 1e-6, "expected 10mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }

    #[test]
    fn dxf_insunits_cm_scales_to_mm() {
        // $INSUNITS = 5 (centimeters); 1 cm -> 10 mm.
        let dxf = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n5\n0\nENDSEC\n\
0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0.0\n20\n0.0\n11\n1.0\n21\n0.0\n0\nENDSEC\n0\nEOF\n";
        let entities = parse_dxf(dxf).unwrap();
        match entities[0].path.subpaths[0].commands[1] {
            PathCommand::LineTo { x, .. } => {
                assert!((x - 10.0).abs() < 1e-6, "expected 10mm, got {x}")
            }
            _ => panic!("expected LineTo"),
        }
    }
}
