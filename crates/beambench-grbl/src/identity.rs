//! Offline, order-independent identity detection for GRBL-family controllers.
//!
//! The detector consumes bounded protocol lines and retains only typed evidence
//! flags plus normalized version candidates. Raw controller text is never kept
//! in the resulting identity or detector state.

use beambench_common::{
    GrblFamilyDialect, GrblFamilyIdentity, GrblFamilyIdentityEvidence, GrblFamilyIdentityStatus,
};

/// Maximum protocol-line size accepted as controller identity evidence.
pub const MAX_IDENTITY_LINE_BYTES: usize = 1024;

/// Accumulates identity evidence without depending on transcript order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GrblFamilyIdentityDetector {
    exact_fluid_nc: bool,
    exact_grbl_hal: bool,
    provisional_fluid_nc: bool,
    provisional_grbl_hal: bool,
    protocol_compatible: bool,
    exact_fluid_nc_versions: VersionCandidates,
    provisional_fluid_nc_versions: VersionCandidates,
    provisional_grbl_hal_versions: VersionCandidates,
    generic_controller_info_versions: VersionCandidates,
    protocol_versions: VersionCandidates,
}

impl GrblFamilyIdentityDetector {
    /// Observe one complete controller-output line.
    ///
    /// Blank, oversized, or lossily decoded lines are ignored fail-closed.
    pub fn observe_line(&mut self, line: &str) {
        if line.len() > MAX_IDENTITY_LINE_BYTES || line.contains('\u{fffd}') {
            return;
        }

        let line = line.trim_matches(|character: char| character.is_ascii_whitespace());
        if line.is_empty() {
            return;
        }

        if line == "[FIRMWARE:grblHAL]" {
            self.exact_grbl_hal = true;
        }

        if let Some(report) = parse_controller_info_version(line) {
            match report {
                ControllerInfoVersion::FluidNc(version) => {
                    self.exact_fluid_nc = true;
                    self.exact_fluid_nc_versions.observe(version);
                }
                ControllerInfoVersion::Generic(version) => {
                    self.generic_controller_info_versions.observe(version);
                }
            }
        }

        if let Some(version) = parse_fluid_nc_startup(line) {
            self.provisional_fluid_nc = true;
            self.provisional_fluid_nc_versions.observe(version);
        }

        if let Some((brand, version)) = parse_grbl_help_banner(line) {
            if brand == "GrblHAL" {
                self.provisional_grbl_hal = true;
                self.provisional_grbl_hal_versions.observe(version);
            } else {
                // A stock-looking banner cannot prove stock GRBL: OEM firmware
                // commonly keeps or rebrands the same protocol help suffix.
                self.protocol_compatible = true;
                self.protocol_versions.observe(version);
            }
        }
    }

    /// Return the identity represented by all evidence observed so far.
    pub fn identity(&self) -> GrblFamilyIdentity {
        let evidence = self.evidence();

        let named_fluid_nc = self.exact_fluid_nc || self.provisional_fluid_nc;
        let named_grbl_hal = self.exact_grbl_hal || self.provisional_grbl_hal;
        if named_fluid_nc && named_grbl_hal {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::Unknown,
                status: GrblFamilyIdentityStatus::Conflicting,
                firmware_identity: None,
                firmware_version: None,
                evidence,
            };
        }

        if self.exact_fluid_nc {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::FluidNc,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: self.exact_fluid_nc_versions.unique(),
                evidence,
            };
        }

        if self.exact_grbl_hal {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::GrblHal,
                status: GrblFamilyIdentityStatus::Identified,
                firmware_identity: Some("grblHAL".to_string()),
                firmware_version: self.generic_controller_info_versions.unique(),
                evidence,
            };
        }

        if self.provisional_fluid_nc {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::FluidNc,
                status: GrblFamilyIdentityStatus::Provisional,
                firmware_identity: Some("FluidNC".to_string()),
                firmware_version: self.provisional_fluid_nc_versions.unique(),
                evidence,
            };
        }

        if self.provisional_grbl_hal {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::GrblHal,
                status: GrblFamilyIdentityStatus::Provisional,
                firmware_identity: Some("grblHAL".to_string()),
                firmware_version: self.provisional_grbl_hal_versions.unique(),
                evidence,
            };
        }

        if self.protocol_compatible {
            return GrblFamilyIdentity {
                dialect: GrblFamilyDialect::Grbl,
                status: GrblFamilyIdentityStatus::ProtocolCompatible,
                firmware_identity: None,
                firmware_version: self.protocol_versions.unique(),
                evidence,
            };
        }

        GrblFamilyIdentity::default()
    }

    fn evidence(&self) -> Vec<GrblFamilyIdentityEvidence> {
        let mut evidence = Vec::with_capacity(4);

        if self.provisional_fluid_nc || self.provisional_grbl_hal {
            evidence.push(GrblFamilyIdentityEvidence::StartupBanner);
        }
        if self.protocol_compatible {
            evidence.push(GrblFamilyIdentityEvidence::ProtocolSignature);
        }
        if self.exact_fluid_nc {
            evidence.push(GrblFamilyIdentityEvidence::ControllerInfoVersion);
        }
        if self.exact_grbl_hal {
            evidence.push(GrblFamilyIdentityEvidence::FirmwareIdentityMessage);
        }

        evidence
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct VersionCandidates {
    value: Option<String>,
    conflicting: bool,
}

impl VersionCandidates {
    fn observe(&mut self, version: Option<String>) {
        let Some(version) = version else {
            return;
        };

        if self.conflicting {
            return;
        }

        match self.value.as_deref() {
            None => self.value = Some(version),
            Some(current) if current == version => {}
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

enum ControllerInfoVersion {
    FluidNc(Option<String>),
    Generic(Option<String>),
}

fn parse_controller_info_version(line: &str) -> Option<ControllerInfoVersion> {
    let body = line.strip_prefix("[VER:")?.strip_suffix(']')?;
    let (preamble, _) = body.split_once(':')?;
    let tokens: Vec<_> = preamble.split_ascii_whitespace().collect();

    let fluid_nc_index = match tokens.as_slice() {
        ["FluidNC", ..] => Some(0),
        [protocol_version, "FluidNC", ..] if is_version_like(protocol_version) => Some(1),
        _ => None,
    };

    if let Some(index) = fluid_nc_index {
        let version = normalize_fluid_nc_build(&tokens[(index + 1)..]);
        return Some(ControllerInfoVersion::FluidNc(version));
    }

    let version = match tokens.as_slice() {
        [version] if is_version_like(version) => normalize_specific_version(version),
        _ => None,
    };
    Some(ControllerInfoVersion::Generic(version))
}

fn parse_fluid_nc_startup(line: &str) -> Option<Option<String>> {
    for prefix in ["[MSG:INFO: FluidNC ", "[MSG: INFO: FluidNC "] {
        if let Some(message) = line.strip_prefix(prefix) {
            if !message.ends_with(']') {
                return None;
            }
            let version = message
                .split_ascii_whitespace()
                .next()
                .and_then(normalize_fluid_nc_version);
            return Some(version);
        }
    }

    let (_, fluid_nc) = line.strip_prefix("Grbl ")?.split_once("[FluidNC ")?;
    if !fluid_nc.ends_with("'$' for help]") {
        return None;
    }

    Some(
        fluid_nc
            .split_ascii_whitespace()
            .next()
            .and_then(normalize_fluid_nc_version),
    )
}

fn parse_grbl_help_banner(line: &str) -> Option<(&str, Option<String>)> {
    let (heading, help) = line.split_once(" [")?;
    if !matches!(help, "'$' for help]" | "'$' or '$HELP' for help]") {
        return None;
    }

    let (brand, version) = heading.rsplit_once(' ')?;
    if brand.is_empty() || !is_version_like(version) {
        return None;
    }

    Some((brand, normalize_specific_version(version)))
}

fn normalize_fluid_nc_version(token: &str) -> Option<String> {
    let version = token.strip_prefix('v')?;
    normalize_release_version(version)
}

fn normalize_fluid_nc_build(build_tokens: &[&str]) -> Option<String> {
    let (release_token, annotations) = build_tokens.split_first()?;
    let release = normalize_fluid_nc_version(release_token)?;

    let (revision_annotations, platform) = match annotations.last() {
        Some(annotation) if is_fluid_nc_platform_annotation(annotation) => {
            (&annotations[..annotations.len() - 1], Some(*annotation))
        }
        _ => (annotations, None),
    };

    let revision = match revision_annotations {
        [] => None,
        [revision] => Some(normalize_revision_annotation(revision)?),
        _ => return None,
    };

    let mut version = release;
    if let Some(revision) = revision {
        version.push(' ');
        version.push_str(&revision);
    }
    if let Some(platform) = platform {
        version.push(' ');
        version.push_str(platform);
    }
    Some(version)
}

fn normalize_release_version(version: &str) -> Option<String> {
    if version.len() > 64
        || !version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'+'))
    {
        return None;
    }

    let core_end = version.find(['-', '+']).unwrap_or(version.len());
    let mut core = version[..core_end].split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (core.next(), core.next(), core.next(), core.next())
    else {
        return None;
    };

    [major, minor, patch]
        .into_iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| version.to_string())
}

fn normalize_revision_annotation(annotation: &str) -> Option<String> {
    let revision = annotation.strip_prefix('(')?.strip_suffix(')')?;
    if revision.eq_ignore_ascii_case("nogit") || revision.eq_ignore_ascii_case("unknown") {
        return None;
    }

    let (revision, dirty) = match revision.strip_suffix("-dirty") {
        Some(revision) => (revision, true),
        None => (revision, false),
    };
    let (branch, commit) = revision.rsplit_once('-')?;
    if branch.is_empty()
        || branch.len() > 96
        || !branch
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'/'))
        || !(7..=40).contains(&commit.len())
        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }

    Some(format!(
        "({branch}-{}{})",
        commit.to_ascii_lowercase(),
        if dirty { "-dirty" } else { "" }
    ))
}

fn is_fluid_nc_platform_annotation(annotation: &str) -> bool {
    matches!(
        annotation,
        "(esp32-wifi)"
            | "(esp32-bt)"
            | "(esp32-noradio)"
            | "(esp32s3-wifi)"
            | "(esp32s3-bt)"
            | "(esp32s3-noradio)"
            | "(win32-capture)"
            | "(test-unit)"
            | "(test-coverage)"
            | "(test-integration)"
    )
}

fn normalize_specific_version(version: &str) -> Option<String> {
    let mut components = version.split('.');
    let (Some(major), Some(protocol), Some(build), None) = (
        components.next(),
        components.next(),
        components.next(),
        components.next(),
    ) else {
        return None;
    };

    (is_version_like(version)
        && !major.is_empty()
        && major.bytes().all(|byte| byte.is_ascii_digit())
        && protocol.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && protocol.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && build.len() == 8
        && build.bytes().all(|byte| byte.is_ascii_digit()))
    .then(|| version.to_string())
}

fn is_version_like(value: &str) -> bool {
    value.len() <= 96
        && value.as_bytes().first().is_some_and(u8::is_ascii_digit)
        && value.bytes().any(|byte| byte == b'.')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_' | b'+' | b'(' | b')')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(lines: &[&str]) -> GrblFamilyIdentity {
        let mut detector = GrblFamilyIdentityDetector::default();
        for line in lines {
            detector.observe_line(line);
        }
        detector.identity()
    }

    #[test]
    fn fluid_nc_requires_token_in_controller_info_preamble() {
        let exact = detect(&["[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :OEM]"]);
        assert_eq!(exact.dialect, GrblFamilyDialect::FluidNc);
        assert_eq!(exact.status, GrblFamilyIdentityStatus::Identified);
        assert_eq!(
            exact.firmware_version.as_deref(),
            Some("4.0.3 (esp32-wifi)")
        );

        for near_miss in [
            "[VER:1.1h.20190830:FluidNC OEM build]",
            "[VER:4.0 Acme FluidNC v4.0.3:OEM]",
            "[VER:4.0 FluidNC-ish v4.0.3:OEM]",
        ] {
            assert_eq!(detect(&[near_miss]), GrblFamilyIdentity::default());
        }
    }

    #[test]
    fn exact_markers_conflict_regardless_of_order() {
        let fluid_nc = "[VER:3.9 FluidNC v3.9.9:OEM]";
        let grbl_hal = "[FIRMWARE:grblHAL]";
        let expected = GrblFamilyIdentity {
            dialect: GrblFamilyDialect::Unknown,
            status: GrblFamilyIdentityStatus::Conflicting,
            firmware_identity: None,
            firmware_version: None,
            evidence: vec![
                GrblFamilyIdentityEvidence::ControllerInfoVersion,
                GrblFamilyIdentityEvidence::FirmwareIdentityMessage,
            ],
        };

        assert_eq!(detect(&[fluid_nc, grbl_hal]), expected);
        assert_eq!(detect(&[grbl_hal, fluid_nc]), expected);
    }

    #[test]
    fn exact_and_provisional_names_conflict_in_both_directions() {
        let exact_fluid_nc = "[VER:4.0 FluidNC v4.0.3 (esp32-wifi) :]";
        let provisional_fluid_nc = "Grbl 4.0 [FluidNC v4.0.3 (esp32-wifi) '$' for help]";
        let exact_grbl_hal = "[FIRMWARE:grblHAL]";
        let provisional_grbl_hal = "GrblHAL 1.1f ['$' or '$HELP' for help]";

        for lines in [
            [exact_fluid_nc, provisional_grbl_hal],
            [provisional_grbl_hal, exact_fluid_nc],
            [provisional_fluid_nc, exact_grbl_hal],
            [exact_grbl_hal, provisional_fluid_nc],
        ] {
            let identity = detect(&lines);
            assert_eq!(identity.dialect, GrblFamilyDialect::Unknown);
            assert_eq!(identity.status, GrblFamilyIdentityStatus::Conflicting);
            assert_eq!(identity.firmware_version, None);
        }
    }

    #[test]
    fn generic_controller_info_needs_grbl_hal_marker() {
        let version = "[VER:1.1f.20260709:OEM FluidNC label]";
        assert_eq!(detect(&[version]), GrblFamilyIdentity::default());

        let identified = detect(&[version, "[FIRMWARE:grblHAL]"]);
        assert_eq!(identified.dialect, GrblFamilyDialect::GrblHal);
        assert_eq!(identified.status, GrblFamilyIdentityStatus::Identified);
        assert_eq!(
            identified.firmware_version.as_deref(),
            Some("1.1f.20260709")
        );

        let conflicting_versions = detect(&[
            "[VER:1.1f.20260708:]",
            "[FIRMWARE:grblHAL]",
            "[VER:1.1f.20260709:]",
        ]);
        assert_eq!(
            conflicting_versions.status,
            GrblFamilyIdentityStatus::Identified
        );
        assert_eq!(conflicting_versions.firmware_version, None);

        for malformed in [
            "[VER:1.1f.x:]",
            "[VER:1.1f.unknown1:]",
            "[VER:1.1f.2026dev:]",
            "[VER:1.1f.20260709.extra:]",
        ] {
            let placeholder = detect(&[malformed, "[FIRMWARE:grblHAL]"]);
            assert_eq!(placeholder.status, GrblFamilyIdentityStatus::Identified);
            assert_eq!(placeholder.firmware_version, None);
        }
    }

    #[test]
    fn exact_versions_win_and_conflicting_versions_fail_closed() {
        let preferred = detect(&[
            "Grbl 3.9 [FluidNC v3.9.8 (wifi) '$' for help]",
            "[VER:3.9 FluidNC v3.9.9:OEM]",
        ]);
        assert_eq!(preferred.firmware_version.as_deref(), Some("3.9.9"));

        let conflicting = detect(&[
            "[VER:3.9 FluidNC v3.9.8:OEM]",
            "[VER:3.9 FluidNC v3.9.9:OEM]",
            "Grbl 3.9 [FluidNC v3.9.7 (wifi) '$' for help]",
        ]);
        assert_eq!(conflicting.status, GrblFamilyIdentityStatus::Identified);
        assert_eq!(conflicting.firmware_version, None);

        let banner_only_version = detect(&[
            "Grbl 3.7 [FluidNC v3.7 (wifi) '$' for help]",
            "[VER:3.7 FluidNC unknown-build:OEM]",
        ]);
        assert_eq!(
            banner_only_version.status,
            GrblFamilyIdentityStatus::Identified
        );
        assert_eq!(banner_only_version.firmware_version, None);

        let grbl_hal_banner_fallback = detect(&[
            "GrblHAL 1.1f ['$' or '$HELP' for help]",
            "[FIRMWARE:grblHAL]",
        ]);
        assert_eq!(
            grbl_hal_banner_fallback.status,
            GrblFamilyIdentityStatus::Identified
        );
        assert_eq!(grbl_hal_banner_fallback.firmware_version, None);
    }

    #[test]
    fn fluid_nc_revision_annotations_remain_distinct_positive_versions() {
        let clean = detect(&["[VER:4.0 FluidNC v4.0.3 (main-94E8ADB) (esp32-wifi) :]"]);
        let dirty =
            detect(&["[VER:4.0 FluidNC v4.0.3 (feature/identity-deadbee-dirty) (esp32-wifi) :]"]);

        assert_eq!(
            clean.firmware_version.as_deref(),
            Some("4.0.3 (main-94e8adb) (esp32-wifi)")
        );
        assert_eq!(
            dirty.firmware_version.as_deref(),
            Some("4.0.3 (feature/identity-deadbee-dirty) (esp32-wifi)")
        );
        assert_ne!(clean.firmware_version, dirty.firmware_version);
        assert_eq!(
            clean
                .positive_identity()
                .and_then(|identity| identity.firmware_version),
            Some("4.0.3 (main-94e8adb) (esp32-wifi)".to_string())
        );
        assert_eq!(
            dirty
                .positive_identity()
                .and_then(|identity| identity.firmware_version),
            Some("4.0.3 (feature/identity-deadbee-dirty) (esp32-wifi)".to_string())
        );

        let bluetooth = detect(&["[VER:4.0 FluidNC v4.0.3 (main-94E8ADB) (esp32-bt) :]"]);
        assert_ne!(clean.firmware_version, bluetooth.firmware_version);
    }

    #[test]
    fn fluid_nc_unknown_builds_identify_model_without_bindable_version() {
        for line in [
            "[VER:4.0 FluidNC v4.0.3 (noGit) (esp32-wifi) :]",
            "[VER:4.0 FluidNC v4.0.3 (unknown) (esp32-wifi) :]",
            "[VER:4.x FluidNC v4.x.x (esp32-wifi) :]",
            "[VER:4.0 FluidNC v4.0 (esp32-wifi) :]",
        ] {
            let identity = detect(&[line]);
            assert_eq!(identity.dialect, GrblFamilyDialect::FluidNc);
            assert_eq!(identity.status, GrblFamilyIdentityStatus::Identified);
            assert_eq!(identity.firmware_version, None);
            assert_eq!(
                identity
                    .positive_identity()
                    .and_then(|positive| positive.firmware_version),
                None
            );
        }
    }

    #[test]
    fn blank_oversized_and_lossy_lines_are_ignored() {
        let oversized = format!(
            "[VER:4.0 FluidNC v4.0.3:{}]",
            "x".repeat(MAX_IDENTITY_LINE_BYTES)
        );
        let lossy = "[VER:4.0 FluidNC v4.0.3:\u{fffd}]";
        let identity = detect(&["", " \r\n ", &oversized, lossy]);
        assert_eq!(identity, GrblFamilyIdentity::default());
    }
}
