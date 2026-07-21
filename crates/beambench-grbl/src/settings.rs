//! GRBL settings storage and accessors.

use std::collections::HashMap;

/// Numeric identifier used by GRBL-family `$N` settings.
///
/// FluidNC and grblHAL define settings above the legacy 8-bit range, so this
/// wire type intentionally covers the full unsigned 16-bit protocol space.
pub type GrblSettingId = u16;

/// Container for GRBL machine settings ($$).
#[derive(Debug, Clone, Default)]
pub struct GrblSettings {
    values: HashMap<GrblSettingId, f64>,
}

impl GrblSettings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a single setting value.
    pub fn set(&mut self, number: GrblSettingId, value: f64) {
        self.values.insert(number, value);
    }

    /// Get a setting value.
    pub fn get(&self, number: GrblSettingId) -> Option<f64> {
        self.values.get(&number).copied()
    }

    /// Number of stored settings.
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Snapshot settings as "$N" -> "value" strings for UI transport.
    pub fn as_string_map(&self) -> HashMap<String, String> {
        self.values
            .iter()
            .map(|(number, value)| (format!("${number}"), value.to_string()))
            .collect()
    }

    // Common setting accessors

    /// $110 — Max rate X (mm/min)
    pub fn max_rate_x(&self) -> Option<f64> {
        self.get(110)
    }

    /// $111 — Max rate Y (mm/min)
    pub fn max_rate_y(&self) -> Option<f64> {
        self.get(111)
    }

    /// $130 — Max travel X (mm)
    pub fn max_travel_x(&self) -> Option<f64> {
        self.get(130)
    }

    /// $131 — Max travel Y (mm)
    pub fn max_travel_y(&self) -> Option<f64> {
        self.get(131)
    }

    /// $32 — Laser mode (0=off, 1=on)
    pub fn laser_mode(&self) -> bool {
        self.get(32).is_some_and(|v| v != 0.0)
    }

    /// $22 — Homing cycle enable
    pub fn homing_enabled(&self) -> bool {
        self.get(22).is_some_and(|v| v != 0.0)
    }

    /// $30 — Max spindle speed (RPM or S-value)
    pub fn max_spindle_speed(&self) -> Option<f64> {
        self.get(30)
    }
}

/// Parse a decimal GRBL-family setting identifier without accepting signs,
/// whitespace, or values outside the shared 16-bit wire contract.
pub fn parse_setting_id(value: &str) -> Option<GrblSettingId> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<GrblSettingId>().ok()
}

/// Parse a single GRBL setting line (e.g., "$110=1000.000").
pub fn parse_setting_line(line: &str) -> Option<(GrblSettingId, f64)> {
    let stripped = line.trim().strip_prefix('$')?;
    let (raw_id, raw_value) = stripped.split_once('=')?;
    let num = parse_setting_id(raw_id)?;
    let val = raw_value.parse::<f64>().ok()?;
    if !val.is_finite() {
        return None;
    }
    Some((num, val))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get_settings() {
        let mut settings = GrblSettings::new();
        settings.set(110, 1000.0);
        settings.set(111, 2000.0);
        assert_eq!(settings.get(110), Some(1000.0));
        assert_eq!(settings.get(111), Some(2000.0));
        assert_eq!(settings.get(112), None);
        assert_eq!(settings.len(), 2);
    }

    #[test]
    fn accessor_methods() {
        let mut settings = GrblSettings::new();
        settings.set(110, 3000.0);
        settings.set(111, 2000.0);
        settings.set(130, 200.0);
        settings.set(131, 300.0);
        settings.set(32, 1.0);
        settings.set(22, 1.0);
        settings.set(30, 1000.0);

        assert_eq!(settings.max_rate_x(), Some(3000.0));
        assert_eq!(settings.max_rate_y(), Some(2000.0));
        assert_eq!(settings.max_travel_x(), Some(200.0));
        assert_eq!(settings.max_travel_y(), Some(300.0));
        assert!(settings.laser_mode());
        assert!(settings.homing_enabled());
        assert_eq!(settings.max_spindle_speed(), Some(1000.0));
    }

    #[test]
    fn laser_mode_defaults_off() {
        let settings = GrblSettings::new();
        assert!(!settings.laser_mode());
    }

    #[test]
    fn parse_setting_line_valid() {
        for (line, expected) in [
            ("$0=10", (0, 10.0)),
            ("$110=1000.000", (110, 1000.0)),
            ("$255=1", (255, 1.0)),
            ("$256=2", (256, 2.0)),
            ("$376=3", (376, 3.0)),
            ("$65535=4", (u16::MAX, 4.0)),
        ] {
            assert_eq!(parse_setting_line(line), Some(expected), "{line}");
        }
    }

    #[test]
    fn parse_setting_line_invalid() {
        assert_eq!(parse_setting_line("ok"), None);
        assert_eq!(parse_setting_line("$abc=1"), None);
        assert_eq!(parse_setting_line("$1=abc"), None);
        assert_eq!(parse_setting_line("$22=NaN"), None);
        assert_eq!(parse_setting_line("$32=inf"), None);
        assert_eq!(parse_setting_line("$30=-inf"), None);
        assert_eq!(parse_setting_line("$65536=1"), None);
        assert_eq!(parse_setting_line("$-1=1"), None);
        assert_eq!(parse_setting_line("$+1=1"), None);
        assert_eq!(parse_setting_line("$ 1=1"), None);
        assert_eq!(parse_setting_line("$1.5=1"), None);
        assert_eq!(parse_setting_line("$=1"), None);
        assert_eq!(parse_setting_line("$1"), None);
        assert_eq!(parse_setting_line("$1=1=2"), None);
    }

    #[test]
    fn as_string_map_uses_dollar_prefixed_keys() {
        let mut settings = GrblSettings::new();
        settings.set(30, 1000.0);
        settings.set(32, 1.0);
        settings.set(376, 3.0);

        let map = settings.as_string_map();
        assert_eq!(map.get("$30"), Some(&"1000".to_string()));
        assert_eq!(map.get("$32"), Some(&"1".to_string()));
        assert_eq!(map.get("$376"), Some(&"3".to_string()));
    }

    #[test]
    fn extended_setting_ids_store_without_truncation() {
        let mut settings = GrblSettings::new();
        settings.set(255, 1.0);
        settings.set(256, 2.0);
        settings.set(376, 3.0);
        settings.set(u16::MAX, 4.0);

        assert_eq!(settings.get(255), Some(1.0));
        assert_eq!(settings.get(256), Some(2.0));
        assert_eq!(settings.get(376), Some(3.0));
        assert_eq!(settings.get(u16::MAX), Some(4.0));
        assert_eq!(settings.len(), 4);
    }
}
