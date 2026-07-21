//! Bounded, order-independent Smoothieware identity detection.

use beambench_common::{ControllerFamily, ControllerModel, PositiveControllerIdentity};
use serde::{Deserialize, Serialize};

/// Maximum controller-output line accepted as identity evidence.
pub const MAX_SMOOTHIEWARE_IDENTITY_LINE_BYTES: usize = 4_096;

/// Maturity of the accumulated Smoothieware identity evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmoothiewareIdentityStatus {
    #[default]
    Unknown,
    Identified,
    OtherFirmware,
    Conflicting,
}

/// Redacted evidence categories retained by the identity result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SmoothiewareIdentityEvidence {
    FirmwareInfo,
    ExtendedFirmwareReport,
    LaserConfiguration,
}

impl SmoothiewareIdentityEvidence {
    const fn description(self) -> &'static str {
        match self {
            Self::FirmwareInfo => "Parsed an exact Smoothieware M115 firmware identity",
            Self::ExtendedFirmwareReport => {
                "Parsed Smoothieware M115 protocol and compatibility fields"
            }
            Self::LaserConfiguration => "Parsed the effective Smoothieware laser configuration",
        }
    }
}

/// Normalized identity accumulated from a read-only Smoothieware `M115` response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SmoothiewareIdentity {
    pub status: SmoothiewareIdentityStatus,
    #[serde(default)]
    pub firmware_identity: Option<String>,
    #[serde(default)]
    pub firmware_version: Option<String>,
    #[serde(default)]
    pub firmware_url: Option<String>,
    #[serde(default)]
    pub source_code_url: Option<String>,
    #[serde(default)]
    pub protocol_version: Option<String>,
    #[serde(default)]
    pub grbl_mode: Option<bool>,
    #[serde(default)]
    pub supports_arcs: Option<bool>,
    #[serde(default)]
    pub laser_module_enabled: Option<bool>,
    #[serde(default)]
    pub laser_maximum_s_value: Option<f64>,
    #[serde(default)]
    pub laser_proportional_power: Option<bool>,
    #[serde(default)]
    pub evidence: Vec<SmoothiewareIdentityEvidence>,
}

impl SmoothiewareIdentity {
    /// Convert exact firmware evidence into the common controller-choice shape.
    pub fn positive_identity(&self) -> Option<PositiveControllerIdentity> {
        if self.status != SmoothiewareIdentityStatus::Identified
            || self.firmware_identity.as_deref() != Some("Smoothieware")
            || !self
                .evidence
                .contains(&SmoothiewareIdentityEvidence::FirmwareInfo)
        {
            return None;
        }

        Some(PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model: ControllerModel::Smoothieware,
            firmware_identity: self.firmware_identity.clone(),
            firmware_version: self.firmware_version.clone(),
            evidence: self
                .evidence
                .iter()
                .map(|item| item.description().to_string())
                .collect(),
        })
    }
}

/// Accumulates M115 response lines without retaining raw controller text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SmoothiewareIdentityDetector {
    saw_smoothieware: bool,
    saw_other_firmware: bool,
    saw_extended_field: bool,
    saw_laser_configuration: bool,
    firmware_versions: StringCandidates,
    firmware_urls: StringCandidates,
    source_code_urls: StringCandidates,
    protocol_versions: StringCandidates,
    grbl_modes: BoolCandidates,
    arc_support: BoolCandidates,
    laser_module_enabled: BoolCandidates,
    laser_maximum_s_values: StringCandidates,
    laser_proportional_power: BoolCandidates,
}

impl SmoothiewareIdentityDetector {
    /// Observe one complete controller-output line.
    ///
    /// Blank, oversized, or lossily decoded lines are ignored. Only an exact
    /// leading `FIRMWARE_NAME:Smoothieware` field establishes positive identity.
    pub fn observe_line(&mut self, line: &str) {
        if line.is_empty()
            || line.len() > MAX_SMOOTHIEWARE_IDENTITY_LINE_BYTES
            || line.contains('\u{FFFD}')
        {
            return;
        }

        if let Some(report) = parse_firmware_report(line) {
            if report.is_smoothieware {
                self.saw_smoothieware = true;
                self.firmware_versions.observe(report.firmware_version);
                self.firmware_urls.observe(report.firmware_url);
                self.source_code_urls.observe(report.source_code_url);
                self.protocol_versions.observe(report.protocol_version);
                self.grbl_modes.observe(report.grbl_mode);
                self.arc_support.observe(report.supports_arcs);
                self.saw_extended_field |= report.saw_extended_field;
            } else {
                self.saw_other_firmware = true;
            }
            return;
        }

        let Some(config) = parse_cached_config(line) else {
            return;
        };
        self.saw_laser_configuration = true;
        match config.setting {
            LaserConfigSetting::Enabled => self
                .laser_module_enabled
                .observe(config.value.map_or(Some(false), parse_config_bool)),
            LaserConfigSetting::MaximumSValue => self.laser_maximum_s_values.observe(
                config
                    .value
                    .map_or_else(|| Some("1.0".to_string()), normalize_positive_number),
            ),
            LaserConfigSetting::ProportionalPower => self
                .laser_proportional_power
                .observe(config.value.map_or(Some(true), parse_config_bool)),
        }
    }

    pub fn identity(&self) -> SmoothiewareIdentity {
        let status = match (self.saw_smoothieware, self.saw_other_firmware) {
            (true, false) => SmoothiewareIdentityStatus::Identified,
            (false, true) => SmoothiewareIdentityStatus::OtherFirmware,
            (true, true) => SmoothiewareIdentityStatus::Conflicting,
            (false, false) => SmoothiewareIdentityStatus::Unknown,
        };
        let identified = status == SmoothiewareIdentityStatus::Identified;

        let mut evidence = Vec::with_capacity(2);
        if self.saw_smoothieware || self.saw_other_firmware {
            evidence.push(SmoothiewareIdentityEvidence::FirmwareInfo);
        }
        if self.saw_extended_field {
            evidence.push(SmoothiewareIdentityEvidence::ExtendedFirmwareReport);
        }
        if self.saw_laser_configuration {
            evidence.push(SmoothiewareIdentityEvidence::LaserConfiguration);
        }

        SmoothiewareIdentity {
            status,
            firmware_identity: identified.then(|| "Smoothieware".to_string()),
            firmware_version: identified
                .then(|| self.firmware_versions.unique())
                .flatten(),
            firmware_url: identified.then(|| self.firmware_urls.unique()).flatten(),
            source_code_url: identified.then(|| self.source_code_urls.unique()).flatten(),
            protocol_version: identified
                .then(|| self.protocol_versions.unique())
                .flatten(),
            grbl_mode: identified.then(|| self.grbl_modes.unique()).flatten(),
            supports_arcs: identified.then(|| self.arc_support.unique()).flatten(),
            laser_module_enabled: identified
                .then(|| self.laser_module_enabled.unique())
                .flatten(),
            laser_maximum_s_value: identified
                .then(|| {
                    self.laser_maximum_s_values
                        .unique()
                        .and_then(|value| value.parse::<f64>().ok())
                })
                .flatten(),
            laser_proportional_power: identified
                .then(|| self.laser_proportional_power.unique())
                .flatten(),
            evidence,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct StringCandidates {
    value: Option<String>,
    conflicting: bool,
}

impl StringCandidates {
    fn observe(&mut self, value: Option<String>) {
        let Some(value) = value else {
            return;
        };
        if self.conflicting {
            return;
        }
        match self.value.as_deref() {
            None => self.value = Some(value),
            Some(current) if current == value => {}
            Some(_) => {
                self.value = None;
                self.conflicting = true;
            }
        }
    }

    fn unique(&self) -> Option<String> {
        (!self.conflicting).then(|| self.value.clone()).flatten()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BoolCandidates {
    value: Option<bool>,
    conflicting: bool,
}

impl BoolCandidates {
    fn observe(&mut self, value: Option<bool>) {
        let Some(value) = value else {
            return;
        };
        if self.conflicting {
            return;
        }
        match self.value {
            None => self.value = Some(value),
            Some(current) if current == value => {}
            Some(_) => {
                self.value = None;
                self.conflicting = true;
            }
        }
    }

    fn unique(&self) -> Option<bool> {
        (!self.conflicting).then_some(self.value).flatten()
    }
}

struct ParsedFirmwareReport {
    is_smoothieware: bool,
    firmware_version: Option<String>,
    firmware_url: Option<String>,
    source_code_url: Option<String>,
    protocol_version: Option<String>,
    grbl_mode: Option<bool>,
    supports_arcs: Option<bool>,
    saw_extended_field: bool,
}

fn parse_firmware_report(line: &str) -> Option<ParsedFirmwareReport> {
    let line = line.trim();
    let fields: Vec<_> = line.split(',').filter_map(parse_field).collect();
    let (first_name, first_value) = fields.first()?;
    if *first_name != "FIRMWARE_NAME" {
        return None;
    }

    let field = |name: &str| {
        fields
            .iter()
            .find_map(|(candidate, value)| (*candidate == name).then_some(*value))
    };
    let normalized = |name: &str, limit| field(name).and_then(|value| normalize(value, limit));
    let boolean = |name: &str| field(name).and_then(parse_bool);

    Some(ParsedFirmwareReport {
        is_smoothieware: *first_value == "Smoothieware",
        firmware_version: normalized("FIRMWARE_VERSION", 128),
        firmware_url: normalized("FIRMWARE_URL", 256),
        source_code_url: normalized("X-SOURCE_CODE_URL", 256),
        protocol_version: normalized("PROTOCOL_VERSION", 64),
        grbl_mode: boolean("X-GRBL_MODE"),
        supports_arcs: boolean("X-ARCS"),
        saw_extended_field: fields.len() > 1,
    })
}

fn parse_field(field: &str) -> Option<(&str, &str)> {
    let (name, value) = field.trim().split_once(':')?;
    let name = name.trim();
    let value = value.trim();
    (!name.is_empty() && !value.is_empty()).then_some((name, value))
}

fn normalize(value: &str, max_len: usize) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && value.len() <= max_len
        && !value.chars().any(char::is_control)
        && !value.contains('\u{FFFD}'))
    .then(|| value.to_string())
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LaserConfigSetting {
    Enabled,
    MaximumSValue,
    ProportionalPower,
}

struct ParsedCachedConfig<'a> {
    setting: LaserConfigSetting,
    value: Option<&'a str>,
}

fn parse_cached_config(line: &str) -> Option<ParsedCachedConfig<'_>> {
    let remainder = line.trim().strip_prefix("cached: ")?;
    let (name, value) = if let Some((name, value)) = remainder.split_once(" is set to ") {
        (name, Some(value.trim()))
    } else {
        (remainder.strip_suffix(" is not in config")?, None)
    };
    let setting = match name.trim() {
        "laser_module_enable" => LaserConfigSetting::Enabled,
        "laser_module_maximum_s_value" => LaserConfigSetting::MaximumSValue,
        "laser_module_proportional_power" => LaserConfigSetting::ProportionalPower,
        _ => return None,
    };
    Some(ParsedCachedConfig { setting, value })
}

fn parse_config_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn normalize_positive_number(value: &str) -> Option<String> {
    let value = value.trim();
    let parsed = value.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_echoes_oversized_lines_and_lossy_text() {
        let mut detector = SmoothiewareIdentityDetector::default();
        detector.observe_line("echo: FIRMWARE_NAME:Smoothieware");
        detector.observe_line(&format!(
            "FIRMWARE_NAME:Smoothieware{}",
            "x".repeat(MAX_SMOOTHIEWARE_IDENTITY_LINE_BYTES)
        ));
        detector.observe_line("FIRMWARE_NAME:Smoothieware\u{FFFD}");
        assert_eq!(
            detector.identity().status,
            SmoothiewareIdentityStatus::Unknown
        );
    }

    #[test]
    fn grbl_mode_does_not_change_the_controller_model() {
        let mut detector = SmoothiewareIdentityDetector::default();
        detector.observe_line("FIRMWARE_NAME:Smoothieware, X-GRBL_MODE:1");
        let identity = detector.identity();
        assert_eq!(identity.grbl_mode, Some(true));
        assert_eq!(
            identity.positive_identity().unwrap().model,
            ControllerModel::Smoothieware
        );
    }

    #[test]
    fn missing_optional_laser_settings_resolve_to_firmware_defaults() {
        let mut detector = SmoothiewareIdentityDetector::default();
        detector.observe_line("FIRMWARE_NAME:Smoothieware");
        detector.observe_line("cached: laser_module_enable is set to true");
        detector.observe_line("cached: laser_module_maximum_s_value is not in config");
        detector.observe_line("cached: laser_module_proportional_power is not in config");

        let identity = detector.identity();
        assert_eq!(identity.laser_module_enabled, Some(true));
        assert_eq!(identity.laser_maximum_s_value, Some(1.0));
        assert_eq!(identity.laser_proportional_power, Some(true));
    }
}
