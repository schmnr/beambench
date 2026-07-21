use serde::{Deserialize, Serialize};

/// A color tag for layers, wrapping an RGBA hex string (e.g. `"#FF0000FF"`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ColorTag(pub String);

impl Default for ColorTag {
    fn default() -> Self {
        Self("#7C6FFFFF".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_color_is_accent() {
        let c = ColorTag::default();
        assert_eq!(c.0, "#7C6FFFFF");
    }

    #[test]
    fn color_roundtrips_through_json() {
        let c = ColorTag("#FF0000FF".to_string());
        let json = serde_json::to_string(&c).unwrap();
        let restored: ColorTag = serde_json::from_str(&json).unwrap();
        assert_eq!(c, restored);
    }
}
