//! G-code parser for import/analysis.

use std::collections::HashMap;

use beambench_common::path::{PathCommand, SubPath, VecPath};
use serde::{Deserialize, Serialize};

/// A single parsed G-code line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GcodeLine {
    pub line_number: usize,
    pub raw: String,
    pub command: Option<String>,
    pub params: HashMap<char, f64>,
}

/// Parse G-code text content into structured lines.
pub fn parse_gcode(content: &str) -> Vec<GcodeLine> {
    content
        .lines()
        .enumerate()
        .map(|(idx, line)| parse_line(idx + 1, line))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MotionMode {
    Rapid,
    Linear,
    Unsupported,
}

/// Convert G-code into open vector paths representing drawable motion.
///
/// This intentionally extracts only geometry. Original machine settings,
/// power commands, feed rates, and other modal details are not preserved.
pub fn import_gcode_as_vecpaths(content: &str) -> Vec<VecPath> {
    let lines = parse_gcode(content);
    let mut position = (0.0, 0.0);
    let mut unit_scale = 1.0;
    let mut absolute_mode = true;
    let mut spindle_on = false;
    let mut power = 0.0;
    let mut motion_mode = MotionMode::Rapid;
    let mut current_subpath: Option<SubPath> = None;
    let mut paths = Vec::new();

    for line in lines {
        if let Some(command) = line.command.as_deref() {
            match command {
                "G0" | "G00" => {
                    motion_mode = MotionMode::Rapid;
                    finish_current_subpath(&mut current_subpath, &mut paths);
                }
                "G1" | "G01" => {
                    motion_mode = MotionMode::Linear;
                }
                "G2" | "G02" | "G3" | "G03" => {
                    motion_mode = MotionMode::Unsupported;
                    finish_current_subpath(&mut current_subpath, &mut paths);
                }
                "G20" => unit_scale = 25.4,
                "G21" => unit_scale = 1.0,
                "G90" => absolute_mode = true,
                "G91" => absolute_mode = false,
                "M3" | "M03" | "M4" | "M04" => spindle_on = true,
                "M5" | "M05" => {
                    spindle_on = false;
                    finish_current_subpath(&mut current_subpath, &mut paths);
                }
                _ => {}
            }
        }

        if let Some(next_power) = line.params.get(&'S').copied() {
            power = next_power;
            if power <= 0.0 {
                finish_current_subpath(&mut current_subpath, &mut paths);
            }
        }

        let has_motion = line.params.contains_key(&'X') || line.params.contains_key(&'Y');
        if !has_motion {
            continue;
        }

        let next_x = line
            .params
            .get(&'X')
            .copied()
            .map(|value| value * unit_scale)
            .map(|value| {
                if absolute_mode {
                    value
                } else {
                    position.0 + value
                }
            })
            .unwrap_or(position.0);
        let next_y = line
            .params
            .get(&'Y')
            .copied()
            .map(|value| value * unit_scale)
            .map(|value| {
                if absolute_mode {
                    value
                } else {
                    position.1 + value
                }
            })
            .unwrap_or(position.1);
        let next_position = (next_x, next_y);

        if next_position == position {
            continue;
        }

        let is_draw = spindle_on && power > 0.0 && motion_mode == MotionMode::Linear;
        if is_draw {
            let subpath = current_subpath.get_or_insert_with(|| {
                let mut subpath = SubPath::new();
                subpath.commands.push(PathCommand::MoveTo {
                    x: position.0,
                    y: position.1,
                });
                subpath
            });
            subpath.commands.push(PathCommand::LineTo {
                x: next_position.0,
                y: next_position.1,
            });
        } else {
            finish_current_subpath(&mut current_subpath, &mut paths);
        }

        position = next_position;
    }

    finish_current_subpath(&mut current_subpath, &mut paths);
    paths
}

/// Parse a single line of G-code.
fn parse_line(line_number: usize, line: &str) -> GcodeLine {
    let raw = line.to_string();

    // Strip comments (everything after semicolon or in parentheses)
    let without_comments = strip_comments(line);
    let trimmed = without_comments.trim();

    if trimmed.is_empty() {
        return GcodeLine {
            line_number,
            raw,
            command: None,
            params: HashMap::new(),
        };
    }

    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    let mut command = None;
    let mut params = HashMap::new();

    for token in tokens {
        if token.is_empty() {
            continue;
        }

        let first_char = token.chars().next().unwrap();

        // Commands start with G or M
        if first_char == 'G' || first_char == 'M' {
            command = Some(token.to_uppercase());
        } else if first_char.is_ascii_alphabetic() {
            // Parameter: letter followed by number
            let letter = first_char.to_ascii_uppercase();
            if let Ok(value) = token[1..].parse::<f64>() {
                params.insert(letter, value);
            }
        }
    }

    GcodeLine {
        line_number,
        raw,
        command,
        params,
    }
}

/// Strip comments from a G-code line.
fn strip_comments(line: &str) -> String {
    // Remove semicolon comments
    let without_semicolon = if let Some(pos) = line.find(';') {
        &line[..pos]
    } else {
        line
    };

    // Remove parenthesis comments
    let mut result = String::new();
    let mut depth: i32 = 0;

    for ch in without_semicolon.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {
                if depth == 0 {
                    result.push(ch);
                }
            }
        }
    }

    result
}

fn finish_current_subpath(current_subpath: &mut Option<SubPath>, paths: &mut Vec<VecPath>) {
    let Some(subpath) = current_subpath.take() else {
        return;
    };
    if subpath.commands.len() < 2 {
        return;
    }
    paths.push(VecPath {
        subpaths: vec![subpath],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_g0_command() {
        let gcode = "G0 X10 Y20";
        let lines = parse_gcode(gcode);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].command, Some("G0".to_string()));
        assert_eq!(lines[0].params.get(&'X'), Some(&10.0));
        assert_eq!(lines[0].params.get(&'Y'), Some(&20.0));
    }

    #[test]
    fn parse_g1_with_feedrate() {
        let gcode = "G1 X5.5 Y10.2 F1000";
        let lines = parse_gcode(gcode);
        assert_eq!(lines[0].command, Some("G1".to_string()));
        assert_eq!(lines[0].params.get(&'X'), Some(&5.5));
        assert_eq!(lines[0].params.get(&'F'), Some(&1000.0));
    }

    #[test]
    fn parse_m_command() {
        let gcode = "M3 S500";
        let lines = parse_gcode(gcode);
        assert_eq!(lines[0].command, Some("M3".to_string()));
        assert_eq!(lines[0].params.get(&'S'), Some(&500.0));
    }

    #[test]
    fn parse_comment_lines() {
        let gcode = "; This is a comment\nG0 X10 ; inline comment";
        let lines = parse_gcode(gcode);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].command, None);
        assert_eq!(lines[1].command, Some("G0".to_string()));
    }

    #[test]
    fn parse_parenthesis_comments() {
        let gcode = "G0 (comment here) X10";
        let lines = parse_gcode(gcode);
        assert_eq!(lines[0].command, Some("G0".to_string()));
        assert_eq!(lines[0].params.get(&'X'), Some(&10.0));
    }

    #[test]
    fn parse_multiple_lines() {
        let gcode = "G0 X0 Y0\nG1 X10 Y10 F500\nM5";
        let lines = parse_gcode(gcode);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].command, Some("G0".to_string()));
        assert_eq!(lines[1].command, Some("G1".to_string()));
        assert_eq!(lines[2].command, Some("M5".to_string()));
    }

    #[test]
    fn import_gcode_as_vecpaths_extracts_drawable_motion() {
        let gcode = "G90\nG0 X0 Y0\nM4 S500\nG1 X10 Y0 F1000\nG1 X10 Y10\nM5\nG0 X20 Y20\nM4 S250\nG1 X30 Y20\nM5";
        let paths = import_gcode_as_vecpaths(gcode);

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].to_svg_d(), "M0 0 L10 0 L10 10");
        assert_eq!(paths[1].to_svg_d(), "M20 20 L30 20");
    }

    #[test]
    fn import_gcode_as_vecpaths_splits_on_non_draw_motion_and_honors_modal_linear_moves() {
        let gcode = "G90\nG0 X0 Y0\nM4 S500\nG1 X10 Y0\nX20 Y0\nG0 X30 Y0\nG1 X40 Y0 S0\nM4 S500\nG1 X50 Y0";
        let paths = import_gcode_as_vecpaths(gcode);

        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].to_svg_d(), "M0 0 L10 0 L20 0");
        assert_eq!(paths[1].to_svg_d(), "M40 0 L50 0");
    }
}
