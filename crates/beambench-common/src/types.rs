//! Core UI and job positioning types.

use serde::{Deserialize, Serialize};

/// Job positioning mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StartFromMode {
    #[default]
    AbsoluteCoords,
    CurrentPosition,
    UserOrigin,
}

/// 3x3 job origin grid
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AnchorPoint {
    #[default]
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Transform locks — Zone S toggles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransformLocks {
    pub move_enabled: bool,
    pub size_enabled: bool,
    pub rotate_enabled: bool,
    pub shear_enabled: bool,
}

impl Default for TransformLocks {
    fn default() -> Self {
        Self {
            move_enabled: true,
            size_enabled: true,
            rotate_enabled: true,
            shear_enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_from_mode_default_is_absolute_coords() {
        assert_eq!(StartFromMode::default(), StartFromMode::AbsoluteCoords);
    }

    #[test]
    fn start_from_mode_roundtrips_through_json() {
        let modes = vec![
            StartFromMode::AbsoluteCoords,
            StartFromMode::CurrentPosition,
            StartFromMode::UserOrigin,
        ];
        for mode in modes {
            let json = serde_json::to_string(&mode).unwrap();
            let restored: StartFromMode = serde_json::from_str(&json).unwrap();
            assert_eq!(mode, restored);
        }
    }

    #[test]
    fn anchor_point_default_is_top_left() {
        assert_eq!(AnchorPoint::default(), AnchorPoint::TopLeft);
    }

    #[test]
    fn anchor_point_roundtrips_through_json() {
        let points = vec![
            AnchorPoint::TopLeft,
            AnchorPoint::TopCenter,
            AnchorPoint::TopRight,
            AnchorPoint::CenterLeft,
            AnchorPoint::Center,
            AnchorPoint::CenterRight,
            AnchorPoint::BottomLeft,
            AnchorPoint::BottomCenter,
            AnchorPoint::BottomRight,
        ];
        for point in points {
            let json = serde_json::to_string(&point).unwrap();
            let restored: AnchorPoint = serde_json::from_str(&json).unwrap();
            assert_eq!(point, restored);
        }
    }

    #[test]
    fn transform_locks_default_all_enabled() {
        let locks = TransformLocks::default();
        assert!(locks.move_enabled);
        assert!(locks.size_enabled);
        assert!(locks.rotate_enabled);
        assert!(locks.shear_enabled);
    }

    #[test]
    fn transform_locks_roundtrips_through_json() {
        let locks = TransformLocks {
            move_enabled: false,
            size_enabled: true,
            rotate_enabled: false,
            shear_enabled: true,
        };
        let json = serde_json::to_string(&locks).unwrap();
        let restored: TransformLocks = serde_json::from_str(&json).unwrap();
        assert_eq!(locks, restored);
    }
}
