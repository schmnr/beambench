//! LightBurn material-library XML import. No machine or library state is changed here.

use crate::{MaterialPreset, OperationType};
use beambench_common::RasterMode;
use roxmltree::Node;
use serde::{Deserialize, Serialize};

pub const MAX_CLB_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub struct ClbImportReport {
    pub presets: Vec<MaterialPreset>,
    pub skipped: Vec<ClbSkippedPreset>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClbSkippedPreset {
    pub name: String,
    pub reason: String,
}

/// Parse the documented material/entry hierarchy and common layer modes.
/// Unsupported settings remain verbatim in notes for review, never as executable code.
pub fn parse_clb_library(xml: &str) -> Result<ClbImportReport, String> {
    if xml.len() > MAX_CLB_BYTES {
        return Err("Material library exceeds the 16 MiB limit".into());
    }
    let doc = roxmltree::Document::parse_with_options(
        xml.trim_start_matches('\u{feff}'),
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 500_000,
        },
    )
    .map_err(|e| format!("Invalid LightBurn material library: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("LightBurnLibrary") {
        return Err("Expected a LightBurn .clb material library (LightBurnLibrary root)".into());
    }
    let mut report = ClbImportReport {
        presets: Vec::new(),
        skipped: Vec::new(),
    };
    for material in root.children().filter(|n| n.has_tag_name("Material")) {
        for entry in material.children().filter(|n| n.has_tag_name("Entry")) {
            if report.presets.len() + report.skipped.len() >= 10_000 {
                return Err("Material library exceeds the 10,000 preset limit".into());
            }
            let material_name = material.attribute("name").unwrap_or("").trim();
            let description = entry.attribute("Desc").unwrap_or("").trim();
            let thickness = entry.attribute("Thickness").unwrap_or("-1");
            let thickness_title = if thickness.parse::<f64>().ok() == Some(-1.0) {
                entry.attribute("NoThickTitle").unwrap_or("").to_owned()
            } else {
                format!("{thickness} mm")
            };
            let name = [material_name, &thickness_title, description]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");
            let settings: Vec<_> = entry
                .children()
                .filter(|n| matches!(n.tag_name().name(), "CutSetting" | "CutSetting_Img"))
                .flat_map(|n| {
                    std::iter::once(n).chain(n.children().filter(|c| c.has_tag_name("SubLayer")))
                })
                .collect();
            if settings.is_empty() {
                report.skipped.push(ClbSkippedPreset {
                    name,
                    reason: "Entry has no cut settings".into(),
                });
                continue;
            }
            for (index, setting) in settings.iter().enumerate() {
                let name = if settings.len() > 1 {
                    format!("{name} / {}", index + 1)
                } else {
                    name.clone()
                };
                match parse_preset(*setting, entry, material_name, &name) {
                    Ok(mut preset) => {
                        if settings.len() > 1 {
                            preset.notes.push_str("\nSub-layers were imported as separate presets. Apply them in the source order; applying one preset does not run the other sub-layers.");
                        }
                        report.presets.push(preset);
                    }
                    Err(reason) => report.skipped.push(ClbSkippedPreset { name, reason }),
                }
                if report.presets.len() + report.skipped.len() > 10_000 {
                    return Err("Material library exceeds the 10,000 preset limit".into());
                }
            }
        }
    }
    if report.presets.is_empty() && report.skipped.is_empty() {
        return Err("LightBurn material library contains no material entries".into());
    }
    Ok(report)
}

fn value<'a>(node: Node<'a, 'a>, key: &str) -> Option<&'a str> {
    node.children()
        .find(|n| n.has_tag_name(key))
        .and_then(|n| n.attribute("Value"))
}

fn number(node: Node<'_, '_>, key: &str, default: f64) -> Result<f64, String> {
    match value(node, key) {
        None if node.children().any(|n| n.has_tag_name(key)) => {
            Err(format!("Missing Value for {key}"))
        }
        None => Ok(default),
        Some(raw) => raw
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("Invalid {key}: {raw}")),
    }
}

fn boolean(node: Node<'_, '_>, key: &str, default: bool) -> Result<bool, String> {
    match value(node, key) {
        None if node.children().any(|n| n.has_tag_name(key)) => {
            Err(format!("Missing Value for {key}"))
        }
        None => Ok(default),
        Some(raw) => match raw.to_ascii_lowercase().as_str() {
            "1" | "true" => Ok(true),
            "0" | "false" => Ok(false),
            _ => Err(format!("Invalid {key}: {raw}")),
        },
    }
}

fn parse_preset(
    setting: Node<'_, '_>,
    entry: Node<'_, '_>,
    material: &str,
    name: &str,
) -> Result<MaterialPreset, String> {
    if material.is_empty() {
        return Err("Material name is missing".into());
    }
    // Disabled parent layers must also disable their sub-layers.
    for parent in setting.ancestors().take_while(|n| *n != entry) {
        if !boolean(parent, "output", true)? || !boolean(parent, "suboutput", true)? {
            return Err("Output is disabled in the source library".into());
        }
    }
    let operation = match setting.attribute("type").unwrap_or("Cut") {
        "Cut" if !setting.has_tag_name("CutSetting_Img") => OperationType::Line,
        "Scan" => OperationType::Fill,
        "Offset" | "OffsetFill" => OperationType::OffsetFill,
        "Image" => OperationType::Image,
        other => return Err(format!("Unsupported operation: {other}")),
    };
    // LightBurn omits values equal to its defaults when writing XML.
    let speed = number(setting, "speed", 100.0)? * 60.0;
    let power = number(setting, "maxPower", 20.0)?;
    let passes = number(setting, "numPasses", 1.0)?;
    if !speed.is_finite() || speed <= 0.0 || !(0.0..=100.0).contains(&power) {
        return Err("Speed must be positive and power must be between 0 and 100 percent".into());
    }
    if passes < 1.0 || passes > u32::MAX as f64 || passes.fract() != 0.0 {
        return Err("Pass count must be a positive whole number".into());
    }
    let thickness = entry
        .attribute("Thickness")
        .unwrap_or("-1")
        .parse::<f64>()
        .map_err(|_| "Invalid material thickness")?;
    if !thickness.is_finite() || (thickness < 0.0 && thickness != -1.0) {
        return Err("Invalid material thickness".into());
    }
    let mut preset = MaterialPreset {
        name: name.into(),
        material: material.into(),
        thickness_mm: thickness.max(0.0),
        operation,
        speed_mm_min: speed,
        power_percent: power,
        passes: passes as u32,
        notes: "Imported from LightBurn .clb. Speed converted from mm/s to mm/min.\nOnly operation, speed, maximum power, passes and supported fill/image settings are applied. Minimum power, air assist, Z, kerf, timing, laser selection and other source settings must be reviewed manually. Omitted source values use LightBurn defaults, including minimum power 20%, air assist on and overscan 2.5%. Original settings follow:".into(),
        ..MaterialPreset::default()
    };
    if thickness == -1.0 {
        preset
            .notes
            .push_str("\nSource has no thickness; imported thickness is 0 mm.");
    }
    if matches!(operation, OperationType::Fill | OperationType::Image) {
        let interval = number(setting, "interval", 0.1)?;
        if interval <= 0.0 {
            return Err("Line interval must be positive".into());
        }
        preset.line_interval_mm = Some(interval);
        preset.scan_angle = Some(number(setting, "angle", 0.0)?);
        preset.bidirectional = Some(boolean(setting, "biDirectional", true)?);
        preset.flood_fill = Some(boolean(setting, "floodFill", false)?);
        preset.angle_passes = Some(if boolean(setting, "crossHatch", false)? {
            2
        } else {
            1
        });
        preset.angle_increment_deg = Some(90.0);
        // Percentage overscan requires the target machine's acceleration. It cannot
        // be translated into a fixed distance without that information.
        if !boolean(setting, "overscan", true)? {
            preset.overscan_mm = Some(0.0);
        }
    }
    if operation == OperationType::Image {
        preset.raster_mode = Some(
            match value(setting, "ditherMode")
                .unwrap_or("stucki")
                .to_ascii_lowercase()
                .as_str()
            {
                "threshold" => RasterMode::Threshold,
                "ordered" => RasterMode::OrderedDither,
                "atkinson" => RasterMode::Atkinson,
                "dither" | "dithered" | "floydsteinberg" => RasterMode::FloydSteinberg,
                "stucki" => RasterMode::Stucki,
                "jarvis" => RasterMode::Jarvis,
                "newsprint" => RasterMode::Newsprint,
                "halftone" => RasterMode::Halftone,
                "sketch" => RasterMode::Sketch,
                "grayscale" => RasterMode::Grayscale,
                other => return Err(format!("Unsupported image mode: {other}")),
            },
        );
        preset.invert = Some(boolean(setting, "negative", false)?);
        preset.pass_through = Some(boolean(setting, "passThrough", false)?);
    }
    for attribute in entry.attributes() {
        preset
            .notes
            .push_str(&format!("\n{}={}", attribute.name(), attribute.value()));
    }
    // Store the exact XML, including unknown values and future extensions. The
    // UI renders this as text and exports retain it in the ordinary notes field.
    preset.notes.push('\n');
    preset
        .notes
        .push_str(&setting.document().input_text()[setting.range()]);
    Ok(preset)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library(settings: &str) -> String {
        format!(
            r#"<LightBurnLibrary><Material name="Birch &amp; Maple"><Entry Thickness="3" Desc="Cut &amp; score">{settings}</Entry></Material></LightBurnLibrary>"#
        )
    }

    #[test]
    fn converts_vector_values_and_preserves_unmapped_settings() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Cut"><speed Value="12.5"/><maxPower Value="65"/><numPasses Value="3"/><kerf Value="0.2"/></CutSetting>"#)).unwrap();
        let p = &report.presets[0];
        assert_eq!(p.material, "Birch & Maple");
        assert!(p.name.contains("Cut & score"));
        assert_eq!(p.thickness_mm, 3.0);
        assert_eq!(p.operation, OperationType::Line);
        assert_eq!(p.speed_mm_min, 750.0);
        assert_eq!(p.power_percent, 65.0);
        assert_eq!(p.passes, 3);
        assert!(p.notes.contains(r#"<kerf Value="0.2"/>"#));
    }

    #[test]
    fn maps_fill_and_lightburn_defaults() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Scan"><interval Value="0.08"/><angle Value="45"/><crossHatch Value="1"/><biDirectional Value="0"/><floodFill Value="1"/><overscan Value="0"/></CutSetting>"#)).unwrap();
        let p = &report.presets[0];
        assert_eq!(
            (p.speed_mm_min, p.power_percent, p.passes),
            (6000.0, 20.0, 1)
        );
        assert_eq!(p.line_interval_mm, Some(0.08));
        assert_eq!(p.scan_angle, Some(45.0));
        assert_eq!(p.angle_passes, Some(2));
        assert_eq!(p.bidirectional, Some(false));
        assert_eq!(p.flood_fill, Some(true));
        assert_eq!(p.overscan_mm, Some(0.0));
    }

    #[test]
    fn splits_sublayers_without_inheriting_sibling_values() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Scan"><maxPower Value="40"/><SubLayer type="Cut" index="1"><speed Value="10"/></SubLayer></CutSetting>"#)).unwrap();
        assert_eq!(report.presets.len(), 2);
        assert!(report.presets[0].name.ends_with(" / 1"));
        assert!(report.presets[1].name.ends_with(" / 2"));
        assert_eq!(report.presets[1].power_percent, 20.0);
        assert!(report.presets[1].notes.contains("source order"));
    }

    #[test]
    fn supports_no_thickness_titles_and_bom() {
        let xml = "\u{feff}<LightBurnLibrary><Material name=\"Slate\"><Entry Thickness=\"-1\" NoThickTitle=\"Coaster\" Desc=\"Engrave\"><CutSetting type=\"Offset\"/></Entry></Material></LightBurnLibrary>";
        let p = &parse_clb_library(xml).unwrap().presets[0];
        assert_eq!(p.name, "Slate / Coaster / Engrave");
        assert_eq!(p.thickness_mm, 0.0);
        assert_eq!(p.operation, OperationType::OffsetFill);
        assert_eq!(p.overscan_mm, None);
    }

    #[test]
    fn reports_unsupported_disabled_and_invalid_entries_without_losing_valid_ones() {
        for invalid in [
            r#"<CutSetting type="Image"><ditherMode Value="3DSlice"/></CutSetting>"#,
            r#"<CutSetting type="Tool"/>"#,
            r#"<CutSetting><speed Value="NaN"/></CutSetting>"#,
            r#"<CutSetting><speed/></CutSetting>"#,
            r#"<CutSetting><maxPower Value="101"/></CutSetting>"#,
            r#"<CutSetting><numPasses Value="1.5"/></CutSetting>"#,
            r#"<CutSetting><interval Value="bad"/><speed Value="0"/></CutSetting>"#,
            r#"<CutSetting><output Value="0"/><SubLayer type="Cut"/></CutSetting>"#,
        ] {
            let report = parse_clb_library(&library(&format!("<CutSetting/>{invalid}"))).unwrap();
            assert_eq!(report.presets.len(), 1, "{invalid}");
            assert!(!report.skipped.is_empty(), "{invalid}");
        }
    }

    #[test]
    fn rejects_wrong_root_malformed_xml_dtd_and_empty_libraries() {
        for xml in [
            "{}",
            "<LightBurnProject/>",
            "<LightBurnLibrary>",
            "<LightBurnLibrary/>",
            "<!DOCTYPE LightBurnLibrary [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><LightBurnLibrary>&x;</LightBurnLibrary>",
        ] {
            assert!(parse_clb_library(xml).is_err(), "{xml}");
        }
        assert!(parse_clb_library(&"x".repeat(MAX_CLB_BYTES + 1)).is_err());
    }

    #[test]
    fn bounds_empty_entry_count_and_rejects_invalid_fill_values() {
        let xml = format!(
            "<LightBurnLibrary><Material name=\"Wood\">{}</Material></LightBurnLibrary>",
            "<Entry/>".repeat(10_001)
        );
        assert!(parse_clb_library(&xml).unwrap_err().contains("10,000"));
        for invalid in [
            r#"<interval Value="0"/>"#,
            r#"<angle Value="inf"/>"#,
            r#"<biDirectional Value="yes"/>"#,
        ] {
            let report = parse_clb_library(&library(&format!(
                "<CutSetting type=\"Scan\">{invalid}</CutSetting>"
            )))
            .unwrap();
            assert!(report.presets.is_empty());
            assert_eq!(report.skipped.len(), 1);
        }
    }

    #[test]
    fn maps_image_mode_and_skips_unknown_algorithms() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Image"><ditherMode Value="jarvis"/><negative Value="1"/><passThrough Value="1"/><interval Value="0.0847"/></CutSetting>"#)).unwrap();
        let p = &report.presets[0];
        assert_eq!(p.operation, OperationType::Image);
        assert_eq!(p.raster_mode, Some(RasterMode::Jarvis));
        assert_eq!(p.invert, Some(true));
        assert_eq!(p.pass_through, Some(true));
        assert_eq!(p.line_interval_mm, Some(0.0847));
        let defaults = parse_clb_library(&library(r#"<CutSetting type="Image"/>"#)).unwrap();
        assert_eq!(defaults.presets[0].raster_mode, Some(RasterMode::Stucki));
    }
}
