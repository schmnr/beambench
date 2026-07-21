//! Bounded, order-independent Marlin identity and capability detection.

use std::collections::BTreeMap;

use beambench_common::{ControllerFamily, ControllerModel, PositiveControllerIdentity};
use serde::{Deserialize, Serialize};

/// Maximum controller-output line accepted as identity or capability evidence.
pub const MAX_MARLIN_IDENTITY_LINE_BYTES: usize = 4_096;

/// Maturity of the accumulated Marlin identity evidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinIdentityStatus {
    #[default]
    Unknown,
    Identified,
    OtherFirmware,
    Conflicting,
}

/// Recognized command dialects among controllers that report Marlin firmware.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinDialect {
    #[default]
    Unknown,
    Generic,
    Snapmaker,
}

/// Redacted evidence categories retained by the identity result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarlinIdentityEvidence {
    FirmwareInfo,
    SnapmakerFirmwareSignature,
    ExtendedCapabilityReport,
}

impl MarlinIdentityEvidence {
    const fn description(self) -> &'static str {
        match self {
            Self::FirmwareInfo => "Parsed Marlin M115 firmware information",
            Self::SnapmakerFirmwareSignature => {
                "Parsed an exact Snapmaker Marlin firmware signature"
            }
            Self::ExtendedCapabilityReport => "Parsed Marlin M115 capability fields",
        }
    }
}

/// Normalized identity accumulated from a read-only Marlin `M115` response.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarlinIdentity {
    pub status: MarlinIdentityStatus,
    #[serde(default)]
    pub dialect: MarlinDialect,
    #[serde(default)]
    pub firmware_identity: Option<String>,
    #[serde(default)]
    pub firmware_version: Option<String>,
    #[serde(default)]
    pub source_code_url: Option<String>,
    #[serde(default)]
    pub protocol_version: Option<String>,
    #[serde(default)]
    pub machine_type: Option<String>,
    #[serde(default)]
    pub capabilities: BTreeMap<String, bool>,
    #[serde(default)]
    pub evidence: Vec<MarlinIdentityEvidence>,
}

impl MarlinIdentity {
    /// Convert exact firmware evidence into the common controller-choice shape.
    pub fn positive_identity(&self) -> Option<PositiveControllerIdentity> {
        if self.status != MarlinIdentityStatus::Identified
            || self.firmware_identity.as_deref() != Some("Marlin")
            || !self
                .evidence
                .contains(&MarlinIdentityEvidence::FirmwareInfo)
        {
            return None;
        }

        let model = match self.dialect {
            MarlinDialect::Generic => ControllerModel::Marlin,
            MarlinDialect::Snapmaker => ControllerModel::Snapmaker,
            MarlinDialect::Unknown => return None,
        };

        Some(PositiveControllerIdentity {
            family: ControllerFamily::Gcode,
            model,
            firmware_identity: self.firmware_identity.clone(),
            firmware_version: self.firmware_version.clone(),
            evidence: self
                .evidence
                .iter()
                .map(|evidence| evidence.description().to_string())
                .collect(),
        })
    }
}

/// Accumulates M115 response lines without retaining raw controller text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarlinIdentityDetector {
    saw_marlin: bool,
    saw_other_firmware: bool,
    saw_snapmaker_signature: bool,
    firmware_versions: StringCandidates,
    source_code_urls: StringCandidates,
    protocol_versions: StringCandidates,
    machine_types: StringCandidates,
    capabilities: BTreeMap<String, BoolCandidates>,
    saw_capability: bool,
}

impl MarlinIdentityDetector {
    /// Observe one complete controller-output line.
    ///
    /// Blank, oversized, or lossily decoded lines are ignored. Only an exact
    /// `FIRMWARE_NAME` field naming Marlin can establish positive identity.
    /// Recognized vendor signatures are retained as distinct dialects.
    pub fn observe_line(&mut self, line: &str) {
        if line.is_empty()
            || line.len() > MAX_MARLIN_IDENTITY_LINE_BYTES
            || line.contains('\u{FFFD}')
        {
            return;
        }

        if let Some(info) = parse_firmware_info(line) {
            match info.firmware {
                FirmwareKind::Marlin { version } => {
                    self.saw_marlin = true;
                    self.saw_snapmaker_signature |=
                        is_snapmaker_signature(version.as_deref(), info.source_code_url.as_deref());
                    self.firmware_versions.observe(version);
                    self.source_code_urls.observe(info.source_code_url);
                    self.protocol_versions.observe(info.protocol_version);
                    self.machine_types.observe(info.machine_type);
                }
                FirmwareKind::Other => self.saw_other_firmware = true,
            }
            return;
        }

        if let Some((name, value)) = parse_capability(line) {
            self.saw_capability = true;
            self.capabilities.entry(name).or_default().observe(value);
        }
    }

    pub fn identity(&self) -> MarlinIdentity {
        let status = match (self.saw_marlin, self.saw_other_firmware) {
            (true, false) => MarlinIdentityStatus::Identified,
            (false, true) => MarlinIdentityStatus::OtherFirmware,
            (true, true) => MarlinIdentityStatus::Conflicting,
            (false, false) => MarlinIdentityStatus::Unknown,
        };
        let identified = status == MarlinIdentityStatus::Identified;

        let dialect = if identified {
            if self.saw_snapmaker_signature {
                MarlinDialect::Snapmaker
            } else {
                MarlinDialect::Generic
            }
        } else {
            MarlinDialect::Unknown
        };

        let mut evidence = Vec::with_capacity(3);
        if self.saw_marlin || self.saw_other_firmware {
            evidence.push(MarlinIdentityEvidence::FirmwareInfo);
        }
        if identified && self.saw_snapmaker_signature {
            evidence.push(MarlinIdentityEvidence::SnapmakerFirmwareSignature);
        }
        if self.saw_capability {
            evidence.push(MarlinIdentityEvidence::ExtendedCapabilityReport);
        }

        MarlinIdentity {
            status,
            dialect,
            firmware_identity: identified.then(|| "Marlin".to_string()),
            firmware_version: identified
                .then(|| self.firmware_versions.unique())
                .flatten(),
            source_code_url: identified.then(|| self.source_code_urls.unique()).flatten(),
            protocol_version: identified
                .then(|| self.protocol_versions.unique())
                .flatten(),
            machine_type: identified.then(|| self.machine_types.unique()).flatten(),
            capabilities: if identified {
                self.capabilities
                    .iter()
                    .filter_map(|(name, candidate)| {
                        candidate.unique().map(|value| (name.clone(), value))
                    })
                    .collect()
            } else {
                BTreeMap::new()
            },
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
    fn observe(&mut self, value: bool) {
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

struct ParsedFirmwareInfo {
    firmware: FirmwareKind,
    source_code_url: Option<String>,
    protocol_version: Option<String>,
    machine_type: Option<String>,
}

enum FirmwareKind {
    Marlin { version: Option<String> },
    Other,
}

fn parse_firmware_info(line: &str) -> Option<ParsedFirmwareInfo> {
    let line = line.trim();
    if !line.starts_with("FIRMWARE_NAME:") {
        return None;
    }
    let fields = parse_fields(line);
    let first = fields.first()?;
    if first.0 != "FIRMWARE_NAME" {
        return None;
    }

    let firmware_name = first.1;
    let mut tokens = firmware_name.split_ascii_whitespace();
    let firmware_token = tokens.next()?;
    let firmware = if firmware_token.eq_ignore_ascii_case("Marlin") {
        FirmwareKind::Marlin {
            version: tokens.next().and_then(normalize_version),
        }
    } else {
        FirmwareKind::Other
    };

    Some(ParsedFirmwareInfo {
        firmware,
        source_code_url: field_value(&fields, "SOURCE_CODE_URL")
            .and_then(|value| normalize_field(value, 256)),
        protocol_version: field_value(&fields, "PROTOCOL_VERSION")
            .and_then(|value| normalize_field(value, 64)),
        machine_type: field_value(&fields, "MACHINE_TYPE")
            .and_then(|value| normalize_field(value, 128)),
    })
}

fn is_snapmaker_signature(version: Option<&str>, source_code_url: Option<&str>) -> bool {
    version.is_some_and(|version| version.to_ascii_lowercase().starts_with("sm2-"))
        || source_code_url.is_some_and(|url| url.to_ascii_lowercase().contains("snapmakermarlin"))
}

fn parse_capability(line: &str) -> Option<(String, bool)> {
    let line = line.trim();
    if line.get(..4)?.eq_ignore_ascii_case("Cap:") {
        let (name, value) = line[4..].split_once(':')?;
        let name = name.trim();
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            return None;
        }
        let value = match value.trim() {
            "0" => false,
            "1" => true,
            _ => return None,
        };
        return Some((name.to_ascii_uppercase(), value));
    }
    None
}

fn normalize_version(value: &str) -> Option<String> {
    let value = value.trim_matches(|character: char| character == ',' || character == ';');
    if value.is_empty()
        || value.len() > 64
        || value.starts_with('(')
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_' | '+')
        })
    {
        return None;
    }
    Some(value.to_string())
}

fn normalize_field(value: &str, max_bytes: usize) -> Option<String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    (!value.is_empty() && value.len() <= max_bytes && value.is_ascii()).then_some(value)
}

fn field_value<'a>(fields: &[(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    fields
        .iter()
        .find_map(|(field, value)| (*field == key).then_some(*value))
}

#[derive(Debug, Clone, Copy)]
struct FieldMarker {
    key_start: usize,
    key_end: usize,
    value_start: usize,
}

fn parse_fields(line: &str) -> Vec<(&str, &str)> {
    let bytes = line.as_bytes();
    let mut markers = Vec::new();
    let mut index = 0;

    while index < bytes.len() {
        let starts_token = index == 0 || bytes[index - 1].is_ascii_whitespace();
        if !starts_token || !bytes[index].is_ascii_uppercase() {
            index += 1;
            continue;
        }

        let key_start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_uppercase()
                || bytes[index].is_ascii_digit()
                || bytes[index] == b'_')
        {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b':' {
            markers.push(FieldMarker {
                key_start,
                key_end: index,
                value_start: index + 1,
            });
        }
        index += 1;
    }

    markers
        .iter()
        .enumerate()
        .map(|(index, marker)| {
            let value_end = markers
                .get(index + 1)
                .map_or(line.len(), |next| next.key_start);
            (
                &line[marker.key_start..marker.key_end],
                line[marker.value_start..value_end].trim(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_firmware_fields_without_splitting_values_on_spaces() {
        let parsed = parse_firmware_info(
            "FIRMWARE_NAME:Marlin 2.1.3 (Jul 17 2026) SOURCE_CODE_URL:github.com/MarlinFirmware/Marlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:Laser Cutter KINEMATICS:Cartesian",
        )
        .unwrap();

        assert!(matches!(parsed.firmware, FirmwareKind::Marlin { .. }));
        assert_eq!(
            parsed.source_code_url.as_deref(),
            Some("github.com/MarlinFirmware/Marlin")
        );
        assert_eq!(parsed.protocol_version.as_deref(), Some("1.0"));
        assert_eq!(parsed.machine_type.as_deref(), Some("Laser Cutter"));
    }

    #[test]
    fn exact_marlin_identity_maps_to_the_shared_model() {
        let mut detector = MarlinIdentityDetector::default();
        detector.observe_line(
            "FIRMWARE_NAME:Marlin 2.1.3 PROTOCOL_VERSION:1.0 MACHINE_TYPE:Laser Cutter",
        );
        detector.observe_line("Cap:EMERGENCY_PARSER:1");

        let identity = detector.identity();
        let positive = identity.positive_identity().unwrap();
        assert_eq!(positive.family, ControllerFamily::Gcode);
        assert_eq!(positive.model, ControllerModel::Marlin);
        assert_eq!(positive.firmware_identity.as_deref(), Some("Marlin"));
        assert_eq!(positive.firmware_version.as_deref(), Some("2.1.3"));
        assert!(positive.is_positive());
    }

    #[test]
    fn exact_snapmaker_signature_maps_to_a_distinct_shared_model() {
        let mut detector = MarlinIdentityDetector::default();
        detector.observe_line(
            "FIRMWARE_NAME:Marlin SM2-4.7.2 (Github) SOURCE_CODE_URL:https://github.com/whimsycwd/SnapmakerMarlin PROTOCOL_VERSION:1.0 MACHINE_TYPE:GD32F305VGT6",
        );

        let identity = detector.identity();
        assert_eq!(identity.dialect, MarlinDialect::Snapmaker);
        assert_eq!(
            identity.source_code_url.as_deref(),
            Some("https://github.com/whimsycwd/SnapmakerMarlin")
        );
        let positive = identity.positive_identity().unwrap();
        assert_eq!(positive.family, ControllerFamily::Gcode);
        assert_eq!(positive.model, ControllerModel::Snapmaker);
        assert!(
            positive
                .evidence
                .iter()
                .any(|evidence| evidence.contains("Snapmaker"))
        );
    }

    #[test]
    fn marlin_named_forks_do_not_count_as_exact_marlin() {
        let mut detector = MarlinIdentityDetector::default();
        detector.observe_line("FIRMWARE_NAME:MarlinFork 9.0 PROTOCOL_VERSION:1.0");

        let identity = detector.identity();
        assert_eq!(identity.status, MarlinIdentityStatus::OtherFirmware);
        assert_eq!(identity.positive_identity(), None);
    }

    #[test]
    fn inconsistent_normalized_identity_cannot_become_positive() {
        let identity = MarlinIdentity {
            status: MarlinIdentityStatus::Identified,
            evidence: vec![MarlinIdentityEvidence::FirmwareInfo],
            ..MarlinIdentity::default()
        };

        assert_eq!(identity.positive_identity(), None);
    }

    #[test]
    fn conflicting_capability_values_are_omitted() {
        let mut detector = MarlinIdentityDetector::default();
        detector.observe_line("FIRMWARE_NAME:Marlin 2.1.3");
        detector.observe_line("Cap:ARCS:1");
        detector.observe_line("Cap:ARCS:0");

        assert!(!detector.identity().capabilities.contains_key("ARCS"));
    }

    #[test]
    fn oversized_and_lossy_lines_are_ignored() {
        let mut detector = MarlinIdentityDetector::default();
        detector.observe_line(&format!(
            "FIRMWARE_NAME:Marlin {}",
            "x".repeat(MAX_MARLIN_IDENTITY_LINE_BYTES)
        ));
        detector.observe_line("FIRMWARE_NAME:Marlin 2.1.3 \u{FFFD}");

        assert_eq!(detector.identity(), MarlinIdentity::default());
    }
}
