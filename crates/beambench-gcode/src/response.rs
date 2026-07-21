use serde::{Deserialize, Serialize};

/// Acknowledgement-based dialect whose response syntax is being classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AcknowledgedGcodeDialect {
    Marlin,
    Smoothieware,
}

/// Transport-neutral meaning of one controller response line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LineProtocolEvent {
    Acknowledged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
    },
    Busy {
        message: String,
    },
    RetryRequested {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_number: Option<u32>,
        message: String,
    },
    CommandError {
        message: String,
    },
    Informational {
        message: String,
    },
}

pub fn classify_response(dialect: AcknowledgedGcodeDialect, line: &str) -> LineProtocolEvent {
    match dialect {
        AcknowledgedGcodeDialect::Marlin => classify_marlin_response(line),
        AcknowledgedGcodeDialect::Smoothieware => classify_smoothieware_response(line),
    }
}

/// Classify a Marlin host-protocol line without changing flow state.
pub fn classify_marlin_response(line: &str) -> LineProtocolEvent {
    let line = line.trim();
    let lower = line.to_ascii_lowercase();

    if let Some(detail) = acknowledgement_detail(line, &lower) {
        return LineProtocolEvent::Acknowledged { detail };
    }
    if lower == "wait" || lower.starts_with("busy:") || lower.starts_with("echo:busy:") {
        return LineProtocolEvent::Busy {
            message: line.to_string(),
        };
    }
    if let Some(remainder) = lower.strip_prefix("resend:") {
        return retry_requested(line, remainder);
    }
    if let Some(remainder) = lower.strip_prefix("rs ") {
        return retry_requested(line, remainder);
    }
    if lower.starts_with("error:") || lower.starts_with("echo:unknown command") {
        return LineProtocolEvent::CommandError {
            message: line.to_string(),
        };
    }

    LineProtocolEvent::Informational {
        message: line.to_string(),
    }
}

/// Classify a Smoothieware ping-pong protocol line without changing flow state.
pub fn classify_smoothieware_response(line: &str) -> LineProtocolEvent {
    let line = line.trim();
    let lower = line.to_ascii_lowercase();

    if lower.starts_with("error:")
        || lower.starts_with("alarm:")
        || lower.starts_with("entering alarm/halt state")
        || lower.starts_with("ok -")
    {
        return LineProtocolEvent::CommandError {
            message: line.to_string(),
        };
    }
    if let Some(detail) = acknowledgement_detail(line, &lower) {
        return LineProtocolEvent::Acknowledged { detail };
    }

    LineProtocolEvent::Informational {
        message: line.to_string(),
    }
}

fn acknowledgement_detail(line: &str, lower: &str) -> Option<Option<String>> {
    if lower == "ok" {
        return Some(None);
    }
    lower.strip_prefix("ok ").map(|_| {
        let detail = line[2..].trim();
        (!detail.is_empty()).then(|| detail.to_string())
    })
}

fn retry_requested(line: &str, remainder: &str) -> LineProtocolEvent {
    LineProtocolEvent::RetryRequested {
        line_number: first_decimal(remainder),
        message: line.to_string(),
    }
}

fn first_decimal(value: &str) -> Option<u32> {
    let start = value.find(|character: char| character.is_ascii_digit())?;
    let digits: String = value[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marlin_advanced_ok_preserves_diagnostic_detail() {
        assert_eq!(
            classify_marlin_response("ok N42 P15 B3"),
            LineProtocolEvent::Acknowledged {
                detail: Some("N42 P15 B3".to_string()),
            }
        );
    }

    #[test]
    fn marlin_resend_line_numbers_are_parsed_when_present() {
        assert_eq!(
            classify_marlin_response("Resend: N123"),
            LineProtocolEvent::RetryRequested {
                line_number: Some(123),
                message: "Resend: N123".to_string(),
            }
        );
        assert_eq!(
            classify_marlin_response("rs 44"),
            LineProtocolEvent::RetryRequested {
                line_number: Some(44),
                message: "rs 44".to_string(),
            }
        );
    }

    #[test]
    fn smoothie_banner_is_not_mistaken_for_an_acknowledgement() {
        assert_eq!(
            classify_smoothieware_response("Smoothie ok"),
            LineProtocolEvent::Informational {
                message: "Smoothie ok".to_string(),
            }
        );
    }

    #[test]
    fn smoothie_invalid_ok_detail_fails_closed() {
        assert_eq!(
            classify_smoothieware_response("ok - Invalid G53"),
            LineProtocolEvent::CommandError {
                message: "ok - Invalid G53".to_string(),
            }
        );
    }
}
