//! Layer color palette with 30 standard colors + 2 tool layers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaletteColor {
    pub index: u8,
    pub name: &'static str,
    pub hex: &'static str,
    pub is_tool_layer: bool,
}

pub const PALETTE_COLORS: [PaletteColor; 32] = [
    // 0-29: Standard layer colors
    PaletteColor {
        index: 0,
        name: "Black",
        hex: "#000000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 1,
        name: "Red",
        hex: "#FF0000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 2,
        name: "Green",
        hex: "#00FF00",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 3,
        name: "Blue",
        hex: "#0000FF",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 4,
        name: "Cyan",
        hex: "#00FFFF",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 5,
        name: "Magenta",
        hex: "#FF00FF",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 6,
        name: "Yellow",
        hex: "#FFFF00",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 7,
        name: "Orange",
        hex: "#FF8000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 8,
        name: "Lilac",
        hex: "#FBB6F0",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 9,
        name: "Sea Green",
        hex: "#2EB88A",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 10,
        name: "Pink",
        hex: "#FF0080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 11,
        name: "Moss",
        hex: "#93B946",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 12,
        name: "Sky Blue",
        hex: "#0080FF",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 13,
        name: "Brown",
        hex: "#804000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 14,
        name: "Maroon",
        hex: "#800000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 15,
        name: "Dark Green",
        hex: "#008000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 16,
        name: "Navy",
        hex: "#000080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 17,
        name: "Olive",
        hex: "#808000",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 18,
        name: "Dark Cyan",
        hex: "#008080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 19,
        name: "Dark Magenta",
        hex: "#800080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 20,
        name: "Coral",
        hex: "#FF8080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 21,
        name: "Pale Green",
        hex: "#D1F0C2",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 22,
        name: "Violet",
        hex: "#987ECE",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 23,
        name: "Sand",
        hex: "#EFCF8F",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 24,
        name: "Steel Blue",
        hex: "#314C81",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 25,
        name: "Plum",
        hex: "#5C2336",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 26,
        name: "Gray",
        hex: "#808080",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 27,
        name: "Light Gray",
        hex: "#C0C0C0",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 28,
        name: "Dark Gray",
        hex: "#404040",
        is_tool_layer: false,
    },
    PaletteColor {
        index: 29,
        name: "Gold",
        hex: "#B8860B",
        is_tool_layer: false,
    },
    // 30-31: Tool layers
    PaletteColor {
        index: 30,
        name: "Tool 1",
        hex: "#DA0B3F",
        is_tool_layer: true,
    },
    PaletteColor {
        index: 31,
        name: "Tool 2",
        hex: "#00D4FF",
        is_tool_layer: true,
    },
];

fn normalize_palette_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_ascii_whitespace() && *ch != '-' && *ch != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

fn parse_palette_index(value: &str) -> Option<u8> {
    let trimmed = value.trim();
    if let Some(rest) = trimmed
        .strip_prefix('T')
        .or_else(|| trimmed.strip_prefix('t'))
        && let Ok(num) = rest.parse::<u8>()
        && (1..=2).contains(&num)
    {
        return Some(29 + num);
    }
    trimmed
        .parse::<u8>()
        .ok()
        .filter(|idx| (*idx as usize) < PALETTE_COLORS.len())
}

fn parse_hex_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().strip_prefix('#')?;
    let rgb = match hex.len() {
        6 => hex,
        8 => &hex[..6],
        _ => return None,
    };
    let r = u8::from_str_radix(&rgb[0..2], 16).ok()?;
    let g = u8::from_str_radix(&rgb[2..4], 16).ok()?;
    let b = u8::from_str_radix(&rgb[4..6], 16).ok()?;
    Some((r, g, b))
}

fn palette_color_from_label(value: &str) -> Option<&'static PaletteColor> {
    if let Some(index) = parse_palette_index(value) {
        return PALETTE_COLORS.get(index as usize);
    }

    let key = normalize_palette_key(value);
    PALETTE_COLORS
        .iter()
        .find(|color| normalize_palette_key(color.name) == key)
}

fn exact_palette_color_from_hex(value: &str) -> Option<&'static PaletteColor> {
    let (r, g, b) = parse_hex_rgb(value)?;
    PALETTE_COLORS.iter().find(|color| {
        parse_hex_rgb(color.hex)
            .map(|candidate| candidate == (r, g, b))
            .unwrap_or(false)
    })
}

fn nearest_standard_palette_color(value: &str) -> Option<&'static PaletteColor> {
    let (r, g, b) = parse_hex_rgb(value)?;
    PALETTE_COLORS
        .iter()
        .filter(|color| !color.is_tool_layer)
        .filter_map(|color| {
            let (cr, cg, cb) = parse_hex_rgb(color.hex)?;
            let dr = r as i32 - cr as i32;
            let dg = g as i32 - cg as i32;
            let db = b as i32 - cb as i32;
            Some((dr * dr + dg * dg + db * db, color))
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, color)| color)
}

/// Returns the canonical palette color for user/agent supplied layer color input.
///
/// Accepted inputs include exact palette hex values (case-insensitive, optional
/// alpha suffix), palette names such as "green" or "dark gray", palette indices
/// like "02", and tool labels "T1"/"T2". Off-palette hex colors are snapped to
/// the nearest standard palette color so layer rows always have a stable label
/// instead of showing as unknown in the UI.
pub fn canonical_palette_color_tag(value: &str) -> &'static str {
    palette_color_from_label(value)
        .or_else(|| exact_palette_color_from_hex(value))
        .or_else(|| nearest_standard_palette_color(value))
        .map(|color| color.hex)
        .unwrap_or(PALETTE_COLORS[0].hex)
}

/// Returns `true` if the given hex color string matches a tool layer in the palette.
pub fn is_tool_color(hex: &str) -> bool {
    exact_palette_color_from_hex(hex).is_some_and(|p| p.is_tool_layer)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPECTED_PALETTE_COLORS: [(u8, &str, &str, bool); 32] = [
        (0, "Black", "#000000", false),
        (1, "Red", "#FF0000", false),
        (2, "Green", "#00FF00", false),
        (3, "Blue", "#0000FF", false),
        (4, "Cyan", "#00FFFF", false),
        (5, "Magenta", "#FF00FF", false),
        (6, "Yellow", "#FFFF00", false),
        (7, "Orange", "#FF8000", false),
        (8, "Lilac", "#FBB6F0", false),
        (9, "Sea Green", "#2EB88A", false),
        (10, "Pink", "#FF0080", false),
        (11, "Moss", "#93B946", false),
        (12, "Sky Blue", "#0080FF", false),
        (13, "Brown", "#804000", false),
        (14, "Maroon", "#800000", false),
        (15, "Dark Green", "#008000", false),
        (16, "Navy", "#000080", false),
        (17, "Olive", "#808000", false),
        (18, "Dark Cyan", "#008080", false),
        (19, "Dark Magenta", "#800080", false),
        (20, "Coral", "#FF8080", false),
        (21, "Pale Green", "#D1F0C2", false),
        (22, "Violet", "#987ECE", false),
        (23, "Sand", "#EFCF8F", false),
        (24, "Steel Blue", "#314C81", false),
        (25, "Plum", "#5C2336", false),
        (26, "Gray", "#808080", false),
        (27, "Light Gray", "#C0C0C0", false),
        (28, "Dark Gray", "#404040", false),
        (29, "Gold", "#B8860B", false),
        (30, "Tool 1", "#DA0B3F", true),
        (31, "Tool 2", "#00D4FF", true),
    ];

    #[test]
    fn palette_matches_expected_metadata_order_exactly() {
        for (color, expected) in PALETTE_COLORS.iter().zip(EXPECTED_PALETTE_COLORS) {
            assert_eq!(color.index, expected.0);
            assert_eq!(color.name, expected.1);
            assert_eq!(color.hex, expected.2);
            assert_eq!(color.is_tool_layer, expected.3);
        }
    }

    #[test]
    fn palette_has_32_colors() {
        assert_eq!(PALETTE_COLORS.len(), 32);
    }

    #[test]
    fn palette_has_30_standard_and_2_tool_colors() {
        let standard_count = PALETTE_COLORS.iter().filter(|c| !c.is_tool_layer).count();
        let tool_count = PALETTE_COLORS.iter().filter(|c| c.is_tool_layer).count();
        assert_eq!(standard_count, 30);
        assert_eq!(tool_count, 2);
    }

    #[test]
    fn palette_indices_are_sequential() {
        for (i, color) in PALETTE_COLORS.iter().enumerate() {
            assert_eq!(color.index as usize, i);
        }
    }

    #[test]
    fn palette_hex_values_are_valid_format() {
        for color in &PALETTE_COLORS {
            assert!(color.hex.starts_with('#'));
            assert_eq!(color.hex.len(), 7);
            // Verify all chars after # are valid hex
            for ch in color.hex[1..].chars() {
                assert!(ch.is_ascii_hexdigit());
            }
        }
    }

    #[test]
    fn is_tool_color_identifies_tool_colors() {
        assert!(is_tool_color("#DA0B3F")); // T1
        assert!(is_tool_color("#da0b3f")); // case insensitive
        assert!(is_tool_color("#DA0B3FFF")); // alpha suffix
        assert!(is_tool_color("#00D4FF")); // T2
    }

    #[test]
    fn is_tool_color_rejects_standard_colors() {
        assert!(!is_tool_color("#000000")); // Black
        assert!(!is_tool_color("#FF0000")); // Red
        assert!(!is_tool_color("#FF8000")); // Orange
        assert!(!is_tool_color("#B8860B")); // Gold
    }

    #[test]
    fn is_tool_color_rejects_unknown_colors() {
        assert!(!is_tool_color("#123456")); // not in palette
        assert!(!is_tool_color("#FF6B00")); // old T1
        assert!(!is_tool_color("#ff6b00")); // old T1, case insensitive
        assert!(!is_tool_color("#ff6b00ff")); // old T1, alpha suffix
    }

    #[test]
    fn canonical_palette_color_accepts_names_indices_and_tool_labels() {
        assert_eq!(canonical_palette_color_tag("green"), "#00FF00");
        assert_eq!(canonical_palette_color_tag("Dark Gray"), "#404040");
        assert_eq!(canonical_palette_color_tag("02"), "#00FF00");
        assert_eq!(canonical_palette_color_tag("orange"), "#FF8000");
        assert_eq!(canonical_palette_color_tag("T1"), "#DA0B3F");
    }

    #[test]
    fn canonical_palette_color_normalizes_hex_and_snaps_unknown_hex() {
        assert_eq!(canonical_palette_color_tag("#00ff00ff"), "#00FF00");
        assert_eq!(canonical_palette_color_tag("#666666"), "#808080");
        assert_eq!(canonical_palette_color_tag("#00aa00"), "#008000");
        assert_eq!(canonical_palette_color_tag("#123456"), "#404040");
        assert_eq!(canonical_palette_color_tag("#FF6B00"), "#FF8000");
        assert_eq!(canonical_palette_color_tag("not-a-color"), "#000000");
    }
}
