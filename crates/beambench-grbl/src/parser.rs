//! GRBL response parser.
//! Parses lines received from GRBL into structured response types.

use beambench_common::machine::{MachinePosition, MachineRunState, MachineStatus};

use crate::settings::{GrblSettingId, parse_setting_line};

/// Parsed GRBL response.
#[derive(Debug, Clone, PartialEq)]
pub enum GrblResponse {
    Ok,
    Error(u8),
    Alarm(u8),
    Status(MachineStatus),
    Banner(String),
    Setting(GrblSettingId, f64),
    Message(String),
    Feedback(String),
    Unknown(String),
}

/// Parse a single line from GRBL into a structured response.
pub fn parse_response(line: &str) -> GrblResponse {
    let trimmed = line.trim();

    if trimmed == "ok" {
        return GrblResponse::Ok;
    }

    if let Some(rest) = trimmed.strip_prefix("error:") {
        if let Ok(code) = rest.trim().parse::<u8>() {
            return GrblResponse::Error(code);
        }
        return GrblResponse::Unknown(trimmed.to_string());
    }

    if let Some(rest) = trimmed.strip_prefix("ALARM:") {
        if let Ok(code) = rest.trim().parse::<u8>() {
            return GrblResponse::Alarm(code);
        }
        return GrblResponse::Unknown(trimmed.to_string());
    }

    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        return GrblResponse::Status(parse_status_report(&trimmed[1..trimmed.len() - 1]));
    }

    // GRBL-protocol startup banners. Stock firmware sends
    // "Grbl 1.1h ['$' for help]", but vendor builds rebrand the name
    // (e.g. "SimpleLaser 1.1h ['$' for help]", "GrblHAL 1.1f [...]").
    // The "['$' for help]" suffix is the protocol signature, so accept a
    // banner on either the name or the suffix.
    if trimmed.to_ascii_lowercase().starts_with("grbl") || trimmed.contains("['$' for help]") {
        return GrblResponse::Banner(trimmed.to_string());
    }

    if trimmed.starts_with('$')
        && let Some(result) = parse_setting_line(trimmed)
    {
        return GrblResponse::Setting(result.0, result.1);
    }

    if trimmed.starts_with("[MSG:") && trimmed.ends_with(']') {
        let msg = &trimmed[5..trimmed.len() - 1];
        return GrblResponse::Message(msg.to_string());
    }

    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let feedback = &trimmed[1..trimmed.len() - 1];
        return GrblResponse::Feedback(feedback.to_string());
    }

    GrblResponse::Unknown(trimmed.to_string())
}

/// Parse a GRBL status report string (without < > delimiters).
pub fn parse_status_report(report: &str) -> MachineStatus {
    let mut status = MachineStatus::default();
    let parts: Vec<&str> = report.split('|').collect();

    if let Some(state_str) = parts.first() {
        status.run_state = parse_run_state(state_str);
    }

    for part in parts.iter().skip(1) {
        if let Some(rest) = part.strip_prefix("MPos:") {
            status.machine_position = parse_position(rest);
        } else if let Some(rest) = part.strip_prefix("WPos:") {
            status.work_position = parse_position(rest);
        } else if let Some(rest) = part.strip_prefix("WCO:") {
            // Work coordinate offset — compute work position from machine position
            let offset = parse_position(rest);
            status.work_position = MachinePosition {
                x: status.machine_position.x - offset.x,
                y: status.machine_position.y - offset.y,
                z: status.machine_position.z - offset.z,
            };
        } else if let Some(rest) = part.strip_prefix("Bf:") {
            // Buffer state: planner,rx — not directly mapped
            let _ = rest;
        } else if let Some(rest) = part.strip_prefix("FS:") {
            let vals: Vec<&str> = rest.split(',').collect();
            if let Some(f) = vals.first() {
                status.feed_rate = f.parse().unwrap_or(0.0);
            }
            if let Some(s) = vals.get(1) {
                status.spindle_speed = s.parse().unwrap_or(0.0);
            }
        } else if let Some(rest) = part.strip_prefix("F:") {
            status.feed_rate = rest.parse().unwrap_or(0.0);
        } else if let Some(rest) = part.strip_prefix("Ov:") {
            let vals: Vec<&str> = rest.split(',').collect();
            if let Some(f) = vals.first() {
                status.feed_override = f.parse().unwrap_or(100);
            }
            if let Some(r) = vals.get(1) {
                status.rapid_override = r.parse().unwrap_or(100);
            }
            if let Some(s) = vals.get(2) {
                status.spindle_override = s.parse().unwrap_or(100);
            }
        } else if let Some(rest) = part.strip_prefix("Pn:") {
            status.pin_states = rest.to_string();
        }
    }

    status
}

/// Parse a GRBL run state string.
pub fn parse_run_state(s: &str) -> MachineRunState {
    match s.trim() {
        "Idle" => MachineRunState::Idle,
        "Run" | "Run:1" | "Run:2" => MachineRunState::Run,
        "Hold" | "Hold:0" | "Hold:1" => MachineRunState::Hold,
        "Jog" => MachineRunState::Jog,
        "Home" => MachineRunState::Home,
        "Alarm" => MachineRunState::Alarm,
        "Door" | "Door:0" | "Door:1" | "Door:2" | "Door:3" => MachineRunState::Door,
        "Sleep" => MachineRunState::Sleep,
        "Check" => MachineRunState::Check,
        state
            if state
                .strip_prefix("Alarm:")
                .is_some_and(|code| code.parse::<u8>().is_ok()) =>
        {
            MachineRunState::Alarm
        }
        _ => MachineRunState::Unknown,
    }
}

fn parse_position(s: &str) -> MachinePosition {
    let coords: Vec<f64> = s.split(',').filter_map(|v| v.parse().ok()).collect();
    MachinePosition {
        x: coords.first().copied().unwrap_or(0.0),
        y: coords.get(1).copied().unwrap_or(0.0),
        z: coords.get(2).copied().unwrap_or(0.0),
    }
}

/// Get a human-readable alarm message for a GRBL alarm code.
pub fn alarm_message(code: u8) -> &'static str {
    match code {
        1 => "Hard limit triggered",
        2 => "Soft limit alarm",
        3 => "Reset while in motion",
        4 => "Probe fail: not cleared before contact",
        5 => "Probe fail: did not contact",
        6 => "Homing fail: reset during homing",
        7 => "Homing fail: door opened during homing",
        8 => "Homing fail: pull off travel failed",
        9 => "Homing fail: could not find limit switch",
        _ => "Unknown alarm",
    }
}

/// Get a human-readable error message for a GRBL error code.
pub fn error_message(code: u8) -> &'static str {
    match code {
        1 => "G-code word expected",
        2 => "Numeric value format error",
        3 => "Invalid $-statement",
        4 => "Negative value for expected positive",
        5 => "Homing cycle not enabled",
        6 => "Minimum step pulse time exceeded",
        7 => "EEPROM read fail",
        8 => "Grbl $-command only valid when idle",
        9 => "G-code locked out during alarm or jog",
        10 => "Soft limits require homing enabled",
        11 => "Line overflow",
        12 => "Max step rate exceeded",
        13 => "Check door",
        14 => "Line length exceeded",
        15 => "Travel exceeded",
        16 => "Invalid jog command",
        17 => "Laser mode requires PWM output",
        20 => "Unsupported command",
        21 => "Modal group violation",
        22 => "Undefined feed rate",
        23 => "Invalid G-code ID",
        24 => "Numeric value invalid",
        25 => "Missing required axis word",
        26 => "Repeated G-code word",
        27 => "G-code axis not configured",
        28 => "Grbl firmware error",
        29 => "Unused word",
        30 => "Jog command with mode change",
        _ => "Unknown error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ok() {
        assert_eq!(parse_response("ok"), GrblResponse::Ok);
    }

    #[test]
    fn parse_error() {
        assert_eq!(parse_response("error:2"), GrblResponse::Error(2));
    }

    #[test]
    fn parse_alarm() {
        assert_eq!(parse_response("ALARM:1"), GrblResponse::Alarm(1));
    }

    #[test]
    fn parse_banner() {
        let resp = parse_response("Grbl 1.1h ['$' for help]");
        match resp {
            GrblResponse::Banner(b) => assert!(b.contains("Grbl 1.1h")),
            _ => panic!("expected banner"),
        }
    }

    #[test]
    fn parse_rebranded_vendor_banners() {
        // Vendor GRBL builds rebrand the name but keep the protocol
        // signature suffix (real-world report: a CH340 board announcing
        // itself as SimpleLaser).
        for line in [
            "SimpleLaser 1.1h ['$' for help]",
            "Ortur Laser Master 2 Ready ['$' for help]",
            "GrblHAL 1.1f ['$' or '$HELP' for help]",
            "grbl 0.9j ['$' for help]",
        ] {
            match parse_response(line) {
                GrblResponse::Banner(b) => assert_eq!(b, line),
                other => panic!("expected banner for {line:?}, got {other:?}"),
            }
        }
        // Feedback/status lines must not be misread as banners.
        assert!(matches!(
            parse_response("[GC:G0 G54 G17]"),
            GrblResponse::Feedback(_)
        ));
    }

    #[test]
    fn parse_setting() {
        for (line, expected) in [
            ("$0=10", GrblResponse::Setting(0, 10.0)),
            ("$255=1", GrblResponse::Setting(255, 1.0)),
            ("$256=2", GrblResponse::Setting(256, 2.0)),
            ("$376=3", GrblResponse::Setting(376, 3.0)),
            ("$65535=4", GrblResponse::Setting(u16::MAX, 4.0)),
        ] {
            assert_eq!(parse_response(line), expected, "{line}");
        }
    }

    #[test]
    fn invalid_setting_lines_are_unknown() {
        for line in [
            "$65536=1", "$-1=1", "$+1=1", "$1.5=1", "$abc=1", "$=1", "$1", "$22=NaN", "$32=inf",
            "$30=-inf",
        ] {
            assert_eq!(
                parse_response(line),
                GrblResponse::Unknown(line.to_string()),
                "{line}"
            );
        }
    }

    #[test]
    fn parse_status_idle() {
        let resp = parse_response("<Idle|MPos:0.000,0.000,0.000|FS:0,0>");
        match resp {
            GrblResponse::Status(s) => {
                assert_eq!(s.run_state, MachineRunState::Idle);
                assert_eq!(s.machine_position.x, 0.0);
                assert_eq!(s.feed_rate, 0.0);
            }
            _ => panic!("expected status"),
        }
    }

    #[test]
    fn parse_status_run_with_position() {
        let resp = parse_response("<Run|MPos:10.500,20.300,0.000|FS:1000,500>");
        match resp {
            GrblResponse::Status(s) => {
                assert_eq!(s.run_state, MachineRunState::Run);
                assert_eq!(s.machine_position.x, 10.5);
                assert_eq!(s.machine_position.y, 20.3);
                assert_eq!(s.feed_rate, 1000.0);
                assert_eq!(s.spindle_speed, 500.0);
            }
            _ => panic!("expected status"),
        }
    }

    #[test]
    fn parse_status_with_overrides() {
        let resp = parse_response("<Idle|MPos:0.000,0.000,0.000|Ov:100,50,80>");
        match resp {
            GrblResponse::Status(s) => {
                assert_eq!(s.feed_override, 100);
                assert_eq!(s.rapid_override, 50);
                assert_eq!(s.spindle_override, 80);
            }
            _ => panic!("expected status"),
        }
    }

    #[test]
    fn parse_message() {
        let resp = parse_response("[MSG:Reset to continue]");
        match resp {
            GrblResponse::Message(m) => assert_eq!(m, "Reset to continue"),
            _ => panic!("expected message"),
        }
    }

    #[test]
    fn parse_feedback() {
        let resp = parse_response("[GC:G0 G54 G17 G21 G90 G94 M5 M9 T0 F0 S0]");
        match resp {
            GrblResponse::Feedback(f) => assert!(f.starts_with("GC:")),
            _ => panic!("expected feedback"),
        }
    }

    #[test]
    fn parse_unknown() {
        let resp = parse_response("something weird");
        assert!(matches!(resp, GrblResponse::Unknown(_)));
    }

    #[test]
    fn run_state_parsing() {
        assert_eq!(parse_run_state("Idle"), MachineRunState::Idle);
        assert_eq!(parse_run_state("Run"), MachineRunState::Run);
        assert_eq!(parse_run_state("Run:2"), MachineRunState::Run);
        assert_eq!(parse_run_state("Hold:0"), MachineRunState::Hold);
        assert_eq!(parse_run_state("Jog"), MachineRunState::Jog);
        assert_eq!(parse_run_state("Home"), MachineRunState::Home);
        assert_eq!(parse_run_state("Alarm"), MachineRunState::Alarm);
        assert_eq!(parse_run_state("Alarm:14"), MachineRunState::Alarm);
        assert_eq!(parse_run_state("Alarm:bad"), MachineRunState::Unknown);
        assert_eq!(parse_run_state("Door:1"), MachineRunState::Door);
        assert_eq!(parse_run_state("Sleep"), MachineRunState::Sleep);
        assert_eq!(parse_run_state("Check"), MachineRunState::Check);
        assert_eq!(parse_run_state("???"), MachineRunState::Unknown);
    }

    #[test]
    fn alarm_messages_known_codes() {
        assert_eq!(alarm_message(1), "Hard limit triggered");
        assert_eq!(alarm_message(9), "Homing fail: could not find limit switch");
        assert_eq!(alarm_message(99), "Unknown alarm");
    }

    #[test]
    fn error_messages_known_codes() {
        assert_eq!(error_message(1), "G-code word expected");
        assert_eq!(error_message(22), "Undefined feed rate");
        assert_eq!(error_message(99), "Unknown error");
    }

    #[test]
    fn parse_status_with_pin_states() {
        let resp = parse_response("<Idle|MPos:0.000,0.000,0.000|Pn:XYZ>");
        match resp {
            GrblResponse::Status(s) => assert_eq!(s.pin_states, "XYZ"),
            _ => panic!("expected status"),
        }
    }
}
