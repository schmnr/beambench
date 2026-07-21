use serde::{Deserialize, Serialize};

/// Workspace origin corner.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceOrigin {
    TopLeft,
    #[default]
    BottomLeft,
}

/// Machine workspace dimensions and coordinate system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    pub bed_width_mm: f64,
    pub bed_height_mm: f64,
    pub origin: WorkspaceOrigin,
}

impl Default for Workspace {
    fn default() -> Self {
        Self {
            bed_width_mm: 400.0,
            bed_height_mm: 400.0,
            origin: WorkspaceOrigin::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_workspace_is_400x400() {
        let ws = Workspace::default();
        assert_eq!(ws.bed_width_mm, 400.0);
        assert_eq!(ws.bed_height_mm, 400.0);
        assert_eq!(ws.origin, WorkspaceOrigin::BottomLeft);
    }

    #[test]
    fn workspace_roundtrips_through_json() {
        let ws = Workspace {
            bed_width_mm: 300.0,
            bed_height_mm: 200.0,
            origin: WorkspaceOrigin::BottomLeft,
        };
        let json = serde_json::to_string(&ws).unwrap();
        let restored: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(ws, restored);
    }

    #[test]
    fn origin_serializes_snake_case() {
        let json = serde_json::to_string(&WorkspaceOrigin::BottomLeft).unwrap();
        assert_eq!(json, "\"bottom_left\"");
    }
}
