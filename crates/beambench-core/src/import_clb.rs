//! LightBurn material-library XML import. No machine or library state is changed here.

use crate::{CutEntry, CutEntryTemplate, MaterialPreset, OperationType};
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
            if settings.len() > 11 {
                report.skipped.push(ClbSkippedPreset {
                    name,
                    reason: "A recipe can have at most 11 sub-layers".into(),
                });
                continue;
            }
            // A source Entry is one complete recipe. Never import a partial stack
            // when an unsupported or invalid sub-layer is found.
            let converted: Result<Vec<_>, _> = settings
                .iter()
                .map(|setting| parse_preset(*setting, entry, material_name, &name))
                .collect();
            match converted {
                Ok(mut presets) => {
                    let mut preset = presets.remove(0);
                    for sublayer in presets {
                        preset.entries.extend(sublayer.entries);
                    }
                    preset.notes = format!(
                        "Imported from LightBurn .clb. Speed converted from mm/s to mm/min.\nThis preset restores the complete sub-layer stack. Machine-specific settings without an equivalent still need manual review. Enabled percentage overscan uses Beam Bench's default distance (2.5 mm); adjust it for your machine. Original entry follows:\n{}",
                        &doc.input_text()[entry.range()]
                    );
                    report.presets.push(preset);
                }
                Err(reason) => report.skipped.push(ClbSkippedPreset { name, reason }),
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
        ..MaterialPreset::default()
    };
    if matches!(
        operation,
        OperationType::Fill | OperationType::Image | OperationType::OffsetFill
    ) {
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
    let mut cut = CutEntry::new(operation);
    cut.speed_mm_min = speed;
    cut.power_percent = power;
    cut.power_min_percent = number(setting, "minPower", 20.0)?;
    if !(0.0..=100.0).contains(&cut.power_min_percent) {
        return Err("Minimum power must be between 0 and 100 percent".into());
    }
    cut.air_assist = boolean(setting, "runBlower", true)?;
    cut.z_offset_mm = number(setting, "zOffset", 0.0)?;
    let layer_setting = setting
        .ancestors()
        .find(|n| matches!(n.tag_name().name(), "CutSetting" | "CutSetting_Img"))
        .unwrap_or(setting);
    cut.output_enabled =
        boolean(layer_setting, "output", true)? && boolean(setting, "suboutput", true)?;
    if let Some(rs) = cut.raster_settings.as_mut() {
        rs.passes = passes as u32;
        rs.line_interval_mm = preset.line_interval_mm.unwrap_or(0.1);
        rs.dpi = (25.4 / rs.line_interval_mm).round() as u32;
        rs.scan_angle = preset.scan_angle.unwrap_or(0.0);
        rs.bidirectional = preset.bidirectional.unwrap_or(true);
        rs.flood_fill = preset.flood_fill.unwrap_or(false);
        rs.angle_passes = preset.angle_passes.unwrap_or(1);
        rs.crosshatch = boolean(setting, "crossHatch", false)?;
        rs.angle_increment_deg = preset.angle_increment_deg.unwrap_or(90.0);
        rs.overscan_mm = preset.overscan_mm.unwrap_or(2.5);
        if let Some(mode) = preset.raster_mode {
            rs.mode = mode;
        }
        rs.invert = preset.invert.unwrap_or(false);
        rs.pass_through = preset.pass_through.unwrap_or(false);
    }
    if let Some(vs) = cut.vector_settings.as_mut() {
        vs.passes = passes as u32;
        vs.kerf_offset_mm = number(setting, "kerf", 0.0)?;
        // LightBurn stores cut/skip distances; Beam Bench stores dwell times.
        // At the imported constant feed, distance / speed gives milliseconds.
        vs.perforation_enabled = boolean(setting, "perforate", false)?;
        if vs.perforation_enabled {
            let on = number(setting, "perfLen", 0.1)?;
            let off = number(setting, "perfSkip", 0.1)?;
            if on <= 0.0 || off < 0.0 {
                return Err("Invalid perforation distance".into());
            }
            vs.perforation_on_ms = on / speed * 60_000.0;
            vs.perforation_off_ms = off / speed * 60_000.0;
            if !vs.perforation_on_ms.is_finite() || !vs.perforation_off_ms.is_finite() {
                return Err("Invalid perforation timing".into());
            }
        }
    }
    preset.entries = vec![CutEntryTemplate::from_entry(&cut)];
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
    fn keeps_sublayers_together_in_order_without_inheriting_sibling_values() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Scan"><maxPower Value="40"/><SubLayer type="Cut" index="1"><speed Value="10"/></SubLayer></CutSetting>"#)).unwrap();
        assert_eq!(report.presets.len(), 1);
        let p = &report.presets[0];
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].operation, OperationType::Fill);
        assert_eq!(p.entries[1].operation, OperationType::Line);
        assert_eq!(p.entries[1].power_percent, 20.0);
        assert_eq!(p.entries[1].speed_mm_min, 600.0);
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
    fn rejects_the_whole_recipe_when_any_entry_is_unsupported_or_invalid() {
        for invalid in [
            r#"<CutSetting type="Image"><ditherMode Value="3DSlice"/></CutSetting>"#,
            r#"<CutSetting type="Tool"/>"#,
            r#"<CutSetting><speed Value="NaN"/></CutSetting>"#,
            r#"<CutSetting><speed/></CutSetting>"#,
            r#"<CutSetting><maxPower Value="101"/></CutSetting>"#,
            r#"<CutSetting><numPasses Value="1.5"/></CutSetting>"#,
            r#"<CutSetting><interval Value="bad"/><speed Value="0"/></CutSetting>"#,
        ] {
            let report = parse_clb_library(&library(&format!("<CutSetting/>{invalid}"))).unwrap();
            assert!(report.presets.is_empty(), "{invalid}");
            assert_eq!(report.skipped.len(), 1, "{invalid}");
        }
    }

    #[test]
    fn preserves_minimum_power_air_z_kerf_perforation_and_disabled_sublayers() {
        let report = parse_clb_library(&library(r#"<CutSetting type="Cut"><speed Value="10"/><minPower Value="12"/><runBlower Value="0"/><zOffset Value="-1.2"/><kerf Value="0.15"/><perforate Value="1"/><perfLen Value="0.4"/><perfSkip Value="0.2"/><suboutput Value="0"/><SubLayer type="Scan"><runBlower Value="1"/></SubLayer></CutSetting>"#)).unwrap();
        let p = &report.presets[0];
        assert_eq!(p.entries.len(), 2);
        let first = &p.entries[0];
        assert_eq!(first.power_min_percent, 12.0);
        assert!(!first.air_assist);
        assert_eq!(first.z_offset_mm, -1.2);
        assert!(!first.output_enabled);
        let vector = first.vector_settings.as_ref().unwrap();
        assert_eq!(vector.kerf_offset_mm, 0.15);
        assert!(!p.notes.contains("does not apply kerf compensation"));
        assert!(vector.perforation_enabled);
        assert!((vector.perforation_on_ms - 40.0).abs() < 1e-10);
        assert!((vector.perforation_off_ms - 20.0).abs() < 1e-10);
        assert!(p.entries[1].air_assist);
        assert!(p.entries[1].output_enabled);
        let disabled = parse_clb_library(&library(
            r#"<CutSetting><output Value="0"/><SubLayer type="Cut"/></CutSetting>"#,
        ))
        .unwrap();
        assert!(
            disabled.presets[0]
                .entries
                .iter()
                .all(|e| !e.output_enabled)
        );
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
