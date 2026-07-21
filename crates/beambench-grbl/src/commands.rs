//! GRBL command builders.
//! Constructs command strings and real-time control bytes for GRBL.

use crate::settings::GrblSettingId;

/// Status query real-time command.
pub fn status_query() -> &'static [u8] {
    b"?"
}

/// Soft reset real-time command (Ctrl-X, 0x18).
pub fn soft_reset() -> &'static [u8] {
    &[0x18]
}

/// Feed hold real-time command.
pub fn feed_hold() -> &'static [u8] {
    b"!"
}

/// Cycle start / resume real-time command.
pub fn cycle_start() -> &'static [u8] {
    b"~"
}

/// Unlock command ($X).
pub fn unlock() -> String {
    "$X".to_string()
}

/// Home all axes ($H).
pub fn home() -> String {
    "$H".to_string()
}

/// Dump all settings ($$).
pub fn settings_dump() -> String {
    "$$".to_string()
}

/// Jog command ($J=).
pub fn jog(x: f64, y: f64, z: Option<f64>, feed: f64) -> String {
    let z_part = z.map(|value| format!("Z{value:.3}")).unwrap_or_default();
    format!("$J=G21G91X{x:.3}Y{y:.3}{z_part}F{feed:.0}")
}

/// Cancel jog (0x85 real-time command).
pub fn jog_cancel() -> &'static [u8] {
    &[0x85]
}

/// Feed override: set to 100%.
pub fn feed_override_reset() -> &'static [u8] {
    &[0x90]
}

/// Feed override: increase by 10%.
pub fn feed_override_increase_10() -> &'static [u8] {
    &[0x91]
}

/// Feed override: decrease by 10%.
pub fn feed_override_decrease_10() -> &'static [u8] {
    &[0x92]
}

/// Feed override: increase by 1%.
pub fn feed_override_increase_1() -> &'static [u8] {
    &[0x93]
}

/// Feed override: decrease by 1%.
pub fn feed_override_decrease_1() -> &'static [u8] {
    &[0x94]
}

/// Spindle override: set to 100%.
pub fn spindle_override_reset() -> &'static [u8] {
    &[0x99]
}

/// Spindle override: increase by 10%.
pub fn spindle_override_increase_10() -> &'static [u8] {
    &[0x9A]
}

/// Spindle override: decrease by 10%.
pub fn spindle_override_decrease_10() -> &'static [u8] {
    &[0x9B]
}

/// Spindle override: increase by 1%.
pub fn spindle_override_increase_1() -> &'static [u8] {
    &[0x9C]
}

/// Spindle override: decrease by 1%.
pub fn spindle_override_decrease_1() -> &'static [u8] {
    &[0x9D]
}

/// Set work coordinate origin (G92) at current position.
pub fn set_origin() -> String {
    "G92 X0 Y0".to_string()
}

/// Reset work coordinate origin (G92.1) to machine coordinates.
pub fn reset_origin() -> String {
    "G92.1".to_string()
}

/// Air assist on command.
pub fn air_on() -> &'static str {
    "M7"
}

/// Air assist off command.
pub fn air_off() -> &'static str {
    "M9"
}

/// Z-axis move command.
pub fn move_z(z_mm: f64, feed: f64) -> String {
    format!("G1 Z{z_mm:.3} F{feed:.0}")
}

/// Absolute work-coordinate move.
pub fn move_to(x: f64, y: f64, z: Option<f64>, feed: f64) -> String {
    let z_part = z.map(|value| format!(" Z{value:.3}")).unwrap_or_default();
    format!("G1 X{x:.3} Y{y:.3}{z_part} F{feed:.0}")
}

/// Absolute machine-coordinate move (G53).
pub fn move_to_machine(x: f64, y: f64, z: Option<f64>, feed: f64) -> String {
    let z_part = z.map(|value| format!(" Z{value:.3}")).unwrap_or_default();
    format!("G53 G0 X{x:.3} Y{y:.3}{z_part} F{feed:.0}")
}

/// Turn laser on at the provided controller-scaled S value.
pub fn laser_fire_on(s_value: u32) -> String {
    format!("M3 S{s_value}")
}

/// Turn laser off.
pub fn laser_fire_off() -> &'static str {
    "M5"
}

/// Send setting command.
pub fn set_setting(key: GrblSettingId, value: f64) -> String {
    format!("${key}={value}")
}

/// Query all settings.
pub fn query_all_settings() -> String {
    "$$".to_string()
}

/// Query controller info.
pub fn controller_info() -> String {
    "$I".to_string()
}

/// Query extended controller info when supported by the GRBL-family firmware.
pub fn extended_controller_info() -> String {
    "$I+".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_query_is_question_mark() {
        assert_eq!(status_query(), b"?");
    }

    #[test]
    fn soft_reset_is_ctrl_x() {
        assert_eq!(soft_reset(), &[0x18]);
    }

    #[test]
    fn jog_formats_correctly() {
        let cmd = jog(10.0, 20.5, None, 1000.0);
        assert_eq!(cmd, "$J=G21G91X10.000Y20.500F1000");
    }

    #[test]
    fn jog_formats_z_when_present() {
        let cmd = jog(10.0, 20.5, Some(-1.25), 1000.0);
        assert_eq!(cmd, "$J=G21G91X10.000Y20.500Z-1.250F1000");
    }

    #[test]
    fn jog_cancel_is_grbl_realtime_byte() {
        assert_eq!(jog_cancel(), &[0x85]);
    }

    #[test]
    fn home_and_unlock_commands() {
        assert_eq!(home(), "$H");
        assert_eq!(unlock(), "$X");
    }

    #[test]
    fn settings_dump_command() {
        assert_eq!(settings_dump(), "$$");
    }

    #[test]
    fn set_origin_commands() {
        assert_eq!(set_origin(), "G92 X0 Y0");
        assert_eq!(reset_origin(), "G92.1");
    }

    #[test]
    fn air_assist_commands() {
        assert_eq!(air_on(), "M7");
        assert_eq!(air_off(), "M9");
    }

    #[test]
    fn move_z_formats_correctly() {
        let cmd = move_z(5.5, 500.0);
        assert_eq!(cmd, "G1 Z5.500 F500");
    }

    #[test]
    fn move_z_negative_value() {
        let cmd = move_z(-2.5, 300.0);
        assert_eq!(cmd, "G1 Z-2.500 F300");
    }

    #[test]
    fn move_to_formats_optional_z() {
        assert_eq!(move_to(1.0, 2.0, None, 3000.0), "G1 X1.000 Y2.000 F3000");
        assert_eq!(
            move_to(1.0, 2.0, Some(3.0), 3000.0),
            "G1 X1.000 Y2.000 Z3.000 F3000"
        );
    }

    #[test]
    fn machine_coordinate_move_formats_g53() {
        assert_eq!(
            move_to_machine(1.0, 2.0, Some(3.0), 3000.0),
            "G53 G0 X1.000 Y2.000 Z3.000 F3000"
        );
    }

    #[test]
    fn fire_commands_format() {
        assert_eq!(laser_fire_on(10), "M3 S10");
        assert_eq!(laser_fire_off(), "M5");
    }

    #[test]
    fn set_setting_formats_correctly() {
        for (key, value, expected) in [
            (0, 10.0, "$0=10"),
            (32, 1.0, "$32=1"),
            (255, 2.0, "$255=2"),
            (256, 3.0, "$256=3"),
            (376, 4.0, "$376=4"),
            (u16::MAX, 5.0, "$65535=5"),
        ] {
            assert_eq!(set_setting(key, value), expected);
        }
    }

    #[test]
    fn query_all_settings_command() {
        assert_eq!(query_all_settings(), "$$");
    }

    #[test]
    fn controller_info_command() {
        assert_eq!(controller_info(), "$I");
        assert_eq!(extended_controller_info(), "$I+");
    }
}
