//! Console logging types for machine communication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsoleEntry {
    pub timestamp: DateTime<Utc>,
    pub direction: ConsoleDirection,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsoleDirection {
    Sent,
    Received,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_direction_roundtrips_through_json() {
        let directions = vec![ConsoleDirection::Sent, ConsoleDirection::Received];
        for direction in directions {
            let json = serde_json::to_string(&direction).unwrap();
            let restored: ConsoleDirection = serde_json::from_str(&json).unwrap();
            assert_eq!(direction, restored);
        }
    }

    #[test]
    fn console_direction_serde_format() {
        let sent = ConsoleDirection::Sent;
        let json = serde_json::to_string(&sent).unwrap();
        assert_eq!(json, "\"sent\"");

        let received = ConsoleDirection::Received;
        let json = serde_json::to_string(&received).unwrap();
        assert_eq!(json, "\"received\"");
    }

    #[test]
    fn console_entry_roundtrips_through_json() {
        let entry = ConsoleEntry {
            timestamp: Utc::now(),
            direction: ConsoleDirection::Sent,
            content: "G0 X10 Y20".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let restored: ConsoleEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, restored);
    }
}
