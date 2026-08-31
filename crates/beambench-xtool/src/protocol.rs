use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M1HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

pub trait M1HttpIo {
    fn get(&mut self, path_and_query: &str) -> Result<M1HttpResponse, String>;
    fn post(
        &mut self,
        path_and_query: &str,
        body: &[u8],
        content_type: &str,
    ) -> Result<M1HttpResponse, String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum M1IdentityConfidence {
    Confirmed,
    ProtocolCompatible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M1Identity {
    pub device_name: Option<String>,
    pub machine_type: Option<String>,
    pub firmware_version: Option<String>,
    pub laser_power_watts: Option<u16>,
    pub confidence: M1IdentityConfidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum M1Status {
    Booting,
    Sleeping,
    Idle,
    ReadyToRun,
    Working,
    Paused,
    Finished,
    Measuring,
    Framing,
    Upgrading,
    Error,
    Unknown(String),
}

impl M1Status {
    pub const fn is_idle(&self) -> bool {
        matches!(self, Self::Sleeping | Self::Idle | Self::Finished)
    }
}

#[derive(Debug, Error)]
pub enum M1ProtocolError {
    #[error("xTool M1 HTTP request failed: {0}")]
    Transport(String),
    #[error("xTool M1 returned HTTP {status} for {path}")]
    HttpStatus { path: String, status: u16 },
    #[error("xTool M1 returned an invalid response for {path}: {message}")]
    InvalidResponse { path: String, message: String },
    #[error("the device answered the M1 protocol but did not provide M1 identity evidence")]
    InconclusiveIdentity,
}

fn checked_body(response: M1HttpResponse, path: &str) -> Result<Vec<u8>, M1ProtocolError> {
    if !(200..300).contains(&response.status) {
        return Err(M1ProtocolError::HttpStatus {
            path: path.to_string(),
            status: response.status,
        });
    }
    Ok(response.body)
}

fn get_body(io: &mut impl M1HttpIo, path: &str) -> Result<Vec<u8>, M1ProtocolError> {
    let response = io.get(path).map_err(M1ProtocolError::Transport)?;
    checked_body(response, path)
}

fn optional_text(io: &mut impl M1HttpIo, path: &str) -> Option<String> {
    let body = get_body(io, path).ok()?;
    let text = String::from_utf8(body).ok()?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn optional_json(io: &mut impl M1HttpIo, path: &str) -> Option<Value> {
    let body = get_body(io, path).ok()?;
    serde_json::from_slice(&body).ok()
}

fn find_string(value: &Value, keys: &[&str]) -> Option<String> {
    match value {
        Value::Object(map) => {
            for key in keys {
                if let Some(value) = map.get(*key) {
                    match value {
                        Value::String(text) if !text.trim().is_empty() => {
                            return Some(text.trim().to_string());
                        }
                        Value::Number(number) => return Some(number.to_string()),
                        _ => {}
                    }
                }
            }
            map.values().find_map(|value| find_string(value, keys))
        }
        Value::Array(values) => values.iter().find_map(|value| find_string(value, keys)),
        _ => None,
    }
}

pub fn parse_m1_status(body: &[u8]) -> Result<M1Status, M1ProtocolError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|error| M1ProtocolError::InvalidResponse {
            path: "/cnc/status".to_string(),
            message: error.to_string(),
        })?;
    let raw = find_string(&value, &["STATUS", "status", "mode", "subMode"]).ok_or_else(|| {
        M1ProtocolError::InvalidResponse {
            path: "/cnc/status".to_string(),
            message: "missing STATUS field".to_string(),
        }
    })?;
    let normalized = raw.trim().to_ascii_uppercase();
    Ok(match normalized.as_str() {
        "P_BOOT" | "BOOT" | "BOOTING" => M1Status::Booting,
        "P_SLEEP" | "SLEEP" => M1Status::Sleeping,
        "P_IDLE" | "IDLE" | "P_WORK" => M1Status::Idle,
        "P_READY" | "P_ONLINE_READY_WORK" | "P_OFFLINE_READY_WORK" => M1Status::ReadyToRun,
        "P_WORKING" | "P_PROCESSING" | "PROCESSING" => M1Status::Working,
        "P_PAUSE" | "P_HUNG" | "PAUSE" | "PAUSED" => M1Status::Paused,
        "P_FINISH" | "P_WORK_DONE" | "FINISHED" | "DONE" => M1Status::Finished,
        "P_MEASURE" | "MEASURING" => M1Status::Measuring,
        "P_FRAMING" | "P_FRAME_READY" | "FRAMING" => M1Status::Framing,
        "P_UPGRADE" | "UPGRADING" => M1Status::Upgrading,
        "P_ERROR" | "ERROR" | "ALARM" => M1Status::Error,
        _ => M1Status::Unknown(raw),
    })
}

pub fn read_m1_status(io: &mut impl M1HttpIo) -> Result<M1Status, M1ProtocolError> {
    let body = get_body(io, "/cnc/status")?;
    parse_m1_status(&body)
}

pub fn probe_m1_identity(io: &mut impl M1HttpIo) -> Result<M1Identity, M1ProtocolError> {
    // Status is required. The other identity calls vary across older firmware.
    let _status = read_m1_status(io)?;
    let device_name = optional_text(io, "/system?action=get_dev_name");
    let machine_type = optional_text(io, "/getmachinetype");
    let version = optional_json(io, "/system?action=version_v2");
    let firmware_version = version.as_ref().and_then(|value| {
        find_string(
            value,
            &[
                "package_version",
                "master_h3_laserservice",
                "master_h3_img",
                "version",
            ],
        )
    });
    let power = optional_json(io, "/getlaserpowertype");
    let laser_power_watts = power
        .as_ref()
        .and_then(|value| find_string(value, &["power", "result", "value"]))
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= u16::MAX as f64)
        .map(|value| value.round() as u16);

    let identity_mentions_m1 = device_name
        .iter()
        .chain(machine_type.iter())
        .any(|value| value.to_ascii_lowercase().contains("m1"));
    let m1_firmware = firmware_version
        .as_deref()
        .is_some_and(|version| version.trim_start_matches(['V', 'v']).starts_with("40.18."));
    let has_identity =
        device_name.is_some() || machine_type.is_some() || firmware_version.is_some();
    if !has_identity {
        return Err(M1ProtocolError::InconclusiveIdentity);
    }

    Ok(M1Identity {
        device_name,
        machine_type,
        firmware_version,
        laser_power_watts,
        confidence: if identity_mentions_m1 || m1_firmware {
            M1IdentityConfidence::Confirmed
        } else {
            M1IdentityConfidence::ProtocolCompatible
        },
    })
}

pub(crate) fn require_ok_response(
    response: M1HttpResponse,
    path: &str,
) -> Result<(), M1ProtocolError> {
    let body = checked_body(response, path)?;
    let text = String::from_utf8_lossy(&body);
    if text.trim().is_empty() || text.trim().eq_ignore_ascii_case("ok") {
        return Ok(());
    }
    let value: Value =
        serde_json::from_slice(&body).map_err(|error| M1ProtocolError::InvalidResponse {
            path: path.to_string(),
            message: error.to_string(),
        })?;
    let result = find_string(&value, &["result", "status"]);
    if result
        .as_deref()
        .is_some_and(|result| result.eq_ignore_ascii_case("ok"))
    {
        Ok(())
    } else {
        Err(M1ProtocolError::InvalidResponse {
            path: path.to_string(),
            message: format!("expected result=ok, received {}", text.trim()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;

    struct ScriptedIo {
        replies: VecDeque<Result<M1HttpResponse, String>>,
    }

    impl M1HttpIo for ScriptedIo {
        fn get(&mut self, _path: &str) -> Result<M1HttpResponse, String> {
            self.replies.pop_front().expect("scripted response")
        }

        fn post(
            &mut self,
            _path: &str,
            _body: &[u8],
            _content_type: &str,
        ) -> Result<M1HttpResponse, String> {
            unreachable!()
        }
    }

    fn ok(body: &str) -> Result<M1HttpResponse, String> {
        Ok(M1HttpResponse {
            status: 200,
            body: body.as_bytes().to_vec(),
        })
    }

    #[test]
    fn parses_legacy_status_values() {
        assert_eq!(
            parse_m1_status(br#"{"STATUS":"P_ONLINE_READY_WORK"}"#).unwrap(),
            M1Status::ReadyToRun
        );
        assert_eq!(
            parse_m1_status(br#"{"STATUS":"P_HUNG"}"#).unwrap(),
            M1Status::Paused
        );
        assert_eq!(
            parse_m1_status(br#"{"STATUS":"P_WORKING"}"#).unwrap(),
            M1Status::Working
        );
    }

    #[test]
    fn preserves_unknown_status_without_guessing() {
        assert_eq!(
            parse_m1_status(br#"{"STATUS":"P_NEW_MODE"}"#).unwrap(),
            M1Status::Unknown("P_NEW_MODE".to_string())
        );
    }

    #[test]
    fn confirms_original_m1_from_firmware_family() {
        let mut io = ScriptedIo {
            replies: VecDeque::from([
                ok(r#"{"STATUS":"P_SLEEP"}"#),
                ok("Workshop M1"),
                ok("M1-10W-123"),
                ok(r#"{"package_version":"40.18.026.00"}"#),
                ok(r#"{"result":"10"}"#),
            ]),
        };
        let identity = probe_m1_identity(&mut io).unwrap();
        assert_eq!(identity.confidence, M1IdentityConfidence::Confirmed);
        assert_eq!(identity.firmware_version.as_deref(), Some("40.18.026.00"));
        assert_eq!(identity.laser_power_watts, Some(10));
    }

    #[test]
    fn accepts_empty_success_body_as_http_acknowledgement() {
        require_ok_response(
            M1HttpResponse {
                status: 200,
                body: Vec::new(),
            },
            "/setprintToolType?type=Laser",
        )
        .unwrap();
    }
}
