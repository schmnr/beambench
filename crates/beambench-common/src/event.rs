use chrono::{DateTime, Utc};
use serde::Serialize;

/// Normalized event envelope emitted from Rust to the frontend.
/// All events share this shape for consistency.
#[derive(Debug, Clone, Serialize)]
pub struct AppEvent<T: Serialize> {
    #[serde(rename = "type")]
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub payload: T,
}

impl<T: Serialize> AppEvent<T> {
    pub fn new(event_type: impl Into<String>, payload: T) -> Self {
        Self {
            event_type: event_type.into(),
            timestamp: Utc::now(),
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_serializes_with_type_field() {
        let event = AppEvent::new("test.event", "hello");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"test.event"#));
        assert!(json.contains(r#""payload":"hello"#));
        assert!(json.contains(r#""timestamp""#));
    }
}
