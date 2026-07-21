//! Project-persisted optimization settings and the patch type used to mutate them.

use serde::{Deserialize, Serialize};

/// Where the laser goes after a job completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FinishPosition {
    #[default]
    Origin,
    DontMove,
    // snake_case would derive "custom_x_y", but the frontend (and the TS
    // FinishPosition type) say "custom_xy" — picking that option in the UI
    // failed with an unknown-variant error. The alias keeps project files
    // saved by builds that serialized "custom_x_y" loading.
    #[serde(rename = "custom_xy", alias = "custom_x_y")]
    CustomXY,
}

/// Deterministic directional bias applied within a sort-key group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DirectionOrder {
    #[default]
    None,
    TopDown,
    BottomUp,
    LeftRight,
    RightLeft,
}

/// Stable sort criteria used by the optimization ordering pass.
///
/// These are not optimization passes. They are ordered keys used to shape the
/// sequence before per-shape and travel optimizers run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationOrderKey {
    Layer,
    Group,
    Priority,
}

fn default_overlap_tolerance() -> f64 {
    0.05
}

fn default_ordering() -> Vec<OptimizationOrderKey> {
    vec![OptimizationOrderKey::Layer, OptimizationOrderKey::Priority]
}

/// Project-persisted optimization settings.
///
/// Project-level path optimization settings. This is a pure data type —
/// runtime-only state (e.g. live machine `current_position`) lives separately in
/// the service-side `OptimizationRuntime` overlay and never crosses the persisted
/// boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectOptimization {
    /// Master toggle for path optimization passes. Output positioning is not
    /// an optimizer pass, so finish-position travel still runs when disabled.
    pub enabled: bool,

    // Hierarchy (applied as outer→inner stable sort keys).
    pub ordering: Vec<OptimizationOrderKey>,

    // Per-shape ordering within a group.
    #[serde(default)]
    pub inner_first: bool,
    #[serde(default)]
    pub direction_order: DirectionOrder,

    // Travel. Default `false` keeps freshly constructed projects conservative;
    // persisted projects must carry the explicit current-schema field set.
    #[serde(default)]
    pub reduce_travel: bool,
    #[serde(default)]
    pub hide_backlash: bool,
    #[serde(default)]
    pub reduce_direction_changes: bool,

    // Per-cut start point.
    #[serde(default)]
    pub choose_best_start: bool,
    #[serde(default)]
    pub choose_corners: bool,
    #[serde(default)]
    pub choose_best_direction: bool,

    // Cleanup.
    #[serde(default)]
    pub remove_overlapping: bool,
    #[serde(default = "default_overlap_tolerance")]
    pub remove_overlap_tolerance_mm: f64,

    // Output positioning (kept — already persisted today).
    #[serde(default)]
    pub start_point_x: Option<f64>,
    #[serde(default)]
    pub start_point_y: Option<f64>,
    #[serde(default)]
    pub finish_position: FinishPosition,
    #[serde(default)]
    pub finish_x: Option<f64>,
    #[serde(default)]
    pub finish_y: Option<f64>,
}

impl Default for ProjectOptimization {
    fn default() -> Self {
        Self {
            enabled: true,
            ordering: default_ordering(),
            inner_first: false,
            direction_order: DirectionOrder::None,
            // See `reduce_travel` field docs above.
            reduce_travel: false,
            hide_backlash: false,
            reduce_direction_changes: false,
            choose_best_start: false,
            choose_corners: false,
            choose_best_direction: false,
            remove_overlapping: false,
            remove_overlap_tolerance_mm: 0.05,
            start_point_x: None,
            start_point_y: None,
            finish_position: FinishPosition::Origin,
            finish_x: None,
            finish_y: None,
        }
    }
}

/// Custom serde helper for `Option<Option<T>>` fields that need to
/// distinguish three wire states:
///   - field absent          → outer `None` (no change)
///   - field present + null  → outer `Some(None)` (explicit clear)
///   - field present + value → outer `Some(Some(value))`
///
/// The stock `Deserialize` derive for `Option<Option<T>>` collapses
/// JSON `null` to the outer `None`, which means a frontend sending
/// `{"start_point_x": null}` to clear the field silently no-ops.
/// `apply_patch` then leaves stale coordinates on disk.
mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T, S>(value: &Option<Option<T>>, ser: S) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
        S: Serializer,
    {
        match value {
            Some(inner) => inner.serialize(ser),
            // Outer None → skip_serializing_if on the field catches this
            // before we ever get called, but be defensive.
            None => ser.serialize_none(),
        }
    }

    pub fn deserialize<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        // If the field is present in JSON, wrap the inner `Option<T>`
        // (which correctly deserializes `null` as `None`) with an outer
        // `Some`. `#[serde(default)]` on the field handles the
        // field-absent case by returning `None`.
        Option::<T>::deserialize(de).map(Some)
    }
}

/// Partial/patch form of [`ProjectOptimization`] used by the frontend mutation API.
///
/// Every field is optional; only set fields cross IPC and merge onto the current
/// project state. This prevents stale full-object overwrites when rapid UI edits
/// race each other — the exact risk the pre-M1 `machineStore.setOptimizationSettings`
/// partial-merge avoided.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectOptimizationPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ordering: Option<Vec<OptimizationOrderKey>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_first: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_order: Option<DirectionOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduce_travel: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_backlash: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reduce_direction_changes: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choose_best_start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choose_corners: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choose_best_direction: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_overlapping: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_overlap_tolerance_mm: Option<f64>,
    /// Outer `Option` = field present in patch; inner `Option` = the field's own nullable value.
    /// The custom `double_option` helper is needed because the derived
    /// `Deserialize` for `Option<Option<T>>` maps JSON `null` to the
    /// outer `None` rather than `Some(None)`, which would silently
    /// discard a "clear to null" patch from the frontend.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub start_point_x: Option<Option<f64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub start_point_y: Option<Option<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_position: Option<FinishPosition>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub finish_x: Option<Option<f64>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "double_option"
    )]
    pub finish_y: Option<Option<f64>>,
}

impl ProjectOptimization {
    pub fn has_order_key(&self, key: OptimizationOrderKey) -> bool {
        self.enabled && self.ordering.contains(&key)
    }

    /// Apply a patch in-place. Returns `true` if any field actually changed.
    /// Callers use the return value for noop short-circuit (no undo snapshot,
    /// no plan-cache invalidation).
    pub fn apply_patch(&mut self, patch: &ProjectOptimizationPatch) -> bool {
        let mut changed = false;

        macro_rules! apply {
            ($field:ident) => {
                if let Some(v) = patch.$field {
                    if self.$field != v {
                        self.$field = v;
                        changed = true;
                    }
                }
            };
        }

        apply!(enabled);
        if let Some(v) = &patch.ordering {
            if self.ordering != *v {
                self.ordering = v.clone();
                changed = true;
            }
        }
        apply!(inner_first);
        apply!(direction_order);
        apply!(reduce_travel);
        apply!(hide_backlash);
        apply!(reduce_direction_changes);
        apply!(choose_best_start);
        apply!(choose_corners);
        apply!(choose_best_direction);
        apply!(remove_overlapping);
        apply!(remove_overlap_tolerance_mm);
        apply!(finish_position);

        // Nullable fields: outer Some means "patch carries this field", inner Option is the new value.
        if let Some(v) = patch.start_point_x {
            if self.start_point_x != v {
                self.start_point_x = v;
                changed = true;
            }
        }
        if let Some(v) = patch.start_point_y {
            if self.start_point_y != v {
                self.start_point_y = v;
                changed = true;
            }
        }
        if let Some(v) = patch.finish_x {
            if self.finish_x != v {
                self.finish_x = v;
                changed = true;
            }
        }
        if let Some(v) = patch.finish_y {
            if self.finish_y != v {
                self.finish_y = v;
                changed = true;
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_preserves_current_structural_behavior() {
        let opt = ProjectOptimization::default();
        assert!(opt.enabled);
        assert!(opt.has_order_key(OptimizationOrderKey::Layer));
        assert!(opt.has_order_key(OptimizationOrderKey::Priority));
        // `reduce_travel` defaults to false for freshly constructed projects.
        assert!(!opt.reduce_travel);
        assert!(!opt.has_order_key(OptimizationOrderKey::Group));
        assert!(!opt.inner_first);
        assert_eq!(opt.direction_order, DirectionOrder::None);
        assert_eq!(opt.finish_position, FinishPosition::Origin);
    }

    #[test]
    fn apply_patch_reports_change() {
        let mut opt = ProjectOptimization::default();
        let patch = ProjectOptimizationPatch {
            inner_first: Some(true),
            ..Default::default()
        };
        assert!(opt.apply_patch(&patch));
        assert!(opt.inner_first);
    }

    #[test]
    fn apply_patch_noop_returns_false() {
        let mut opt = ProjectOptimization::default();
        let patch = ProjectOptimizationPatch {
            enabled: Some(true), // already true
            ..Default::default()
        };
        assert!(!opt.apply_patch(&patch));
    }

    #[test]
    fn apply_patch_empty_returns_false() {
        let mut opt = ProjectOptimization::default();
        let patch = ProjectOptimizationPatch::default();
        assert!(!opt.apply_patch(&patch));
    }

    #[test]
    fn default_ordering_is_layer_then_priority() {
        let opt = ProjectOptimization::default();
        assert_eq!(
            opt.ordering,
            vec![OptimizationOrderKey::Layer, OptimizationOrderKey::Priority]
        );
        assert!(!opt.has_order_key(OptimizationOrderKey::Group));
    }

    #[test]
    fn apply_patch_clears_nullable_field() {
        let mut opt = ProjectOptimization {
            start_point_x: Some(10.0),
            ..Default::default()
        };
        let patch = ProjectOptimizationPatch {
            start_point_x: Some(None), // outer Some = "I'm setting it", inner None = "to null"
            ..Default::default()
        };
        assert!(opt.apply_patch(&patch));
        assert!(opt.start_point_x.is_none());
    }

    #[test]
    fn apply_patch_preserves_sibling_fields() {
        let mut opt = ProjectOptimization {
            inner_first: true,
            reduce_travel: true,
            ordering: vec![OptimizationOrderKey::Layer, OptimizationOrderKey::Group],
            ..Default::default()
        };
        let patch = ProjectOptimizationPatch {
            inner_first: Some(false),
            ..Default::default()
        };
        opt.apply_patch(&patch);
        // Sibling fields untouched.
        assert!(opt.reduce_travel);
        assert!(opt.has_order_key(OptimizationOrderKey::Group));
        assert!(!opt.inner_first);
    }

    #[test]
    fn patch_serialization_omits_unset_fields() {
        let patch = ProjectOptimizationPatch {
            inner_first: Some(true),
            ..Default::default()
        };
        let json = serde_json::to_string(&patch).unwrap();
        // Only the set field should serialize.
        assert!(json.contains("inner_first"));
        assert!(!json.contains("ordering"));
        assert!(!json.contains("reduce_travel"));
    }

    #[test]
    fn project_optimization_requires_current_wire_fields() {
        let json = "{}";
        let err = serde_json::from_str::<ProjectOptimization>(json).unwrap_err();
        assert!(err.to_string().contains("enabled"));
    }

    #[test]
    fn project_optimization_roundtrip() {
        let opt = ProjectOptimization {
            inner_first: true,
            direction_order: DirectionOrder::TopDown,
            choose_best_start: true,
            choose_corners: true,
            remove_overlapping: true,
            remove_overlap_tolerance_mm: 0.1,
            finish_position: FinishPosition::CustomXY,
            finish_x: Some(50.0),
            finish_y: Some(100.0),
            ..Default::default()
        };
        let json = serde_json::to_string(&opt).unwrap();
        let restored: ProjectOptimization = serde_json::from_str(&json).unwrap();
        assert_eq!(opt, restored);
    }

    #[test]
    fn finish_position_default_is_origin() {
        assert_eq!(FinishPosition::default(), FinishPosition::Origin);
    }

    #[test]
    fn finish_position_custom_wire_format_matches_frontend() {
        // The UI sends "custom_xy"; rejecting it broke the finish-position
        // dropdown with an unknown-variant error (report r-3eecba12).
        assert_eq!(
            serde_json::to_string(&FinishPosition::CustomXY).unwrap(),
            "\"custom_xy\""
        );
        assert_eq!(
            serde_json::from_str::<FinishPosition>("\"custom_xy\"").unwrap(),
            FinishPosition::CustomXY
        );
        // Builds before the rename serialized "custom_x_y" into saved
        // projects; those files must keep loading.
        assert_eq!(
            serde_json::from_str::<FinishPosition>("\"custom_x_y\"").unwrap(),
            FinishPosition::CustomXY
        );
    }

    #[test]
    fn direction_order_default_is_none() {
        assert_eq!(DirectionOrder::default(), DirectionOrder::None);
    }

    #[test]
    fn patch_null_nullable_field_deserializes_as_some_none() {
        // Regression for the Option<Option<T>> wire-format issue: a JSON
        // `null` for a nullable coordinate field must decode to
        // `Some(None)` (explicit clear), not `None` (field absent).
        // Without the `double_option` helper, serde's stock derive
        // collapses JSON `null` to the outer `None`, which silently
        // no-ops `apply_patch` and leaves stale coordinates on disk.
        let patch: ProjectOptimizationPatch =
            serde_json::from_str(r#"{"start_point_x": null}"#).unwrap();
        assert_eq!(patch.start_point_x, Some(None));
        assert_eq!(patch.start_point_y, None); // field absent → outer None

        let patch: ProjectOptimizationPatch =
            serde_json::from_str(r#"{"finish_x": null, "finish_y": null}"#).unwrap();
        assert_eq!(patch.finish_x, Some(None));
        assert_eq!(patch.finish_y, Some(None));
    }

    #[test]
    fn patch_value_nullable_field_deserializes_as_some_some() {
        let patch: ProjectOptimizationPatch =
            serde_json::from_str(r#"{"start_point_x": 12.5}"#).unwrap();
        assert_eq!(patch.start_point_x, Some(Some(12.5)));
    }

    #[test]
    fn apply_patch_clears_coordinates_from_json_null() {
        // End-to-end: frontend sends `{"start_point_x": null}` to clear
        // a previously-set custom start point. The patch must survive
        // round-trip deserialization and apply_patch must flip the
        // stored coordinate to None.
        let mut opt = ProjectOptimization {
            start_point_x: Some(42.0),
            start_point_y: Some(99.0),
            finish_position: FinishPosition::CustomXY,
            finish_x: Some(50.0),
            finish_y: Some(60.0),
            ..Default::default()
        };
        let patch: ProjectOptimizationPatch = serde_json::from_str(
            r#"{"start_point_x": null, "start_point_y": null, "finish_x": null, "finish_y": null}"#,
        )
        .unwrap();
        assert!(opt.apply_patch(&patch));
        assert!(opt.start_point_x.is_none());
        assert!(opt.start_point_y.is_none());
        assert!(opt.finish_x.is_none());
        assert!(opt.finish_y.is_none());
    }

    #[test]
    fn patch_absent_field_leaves_state_unchanged() {
        // Sanity check: omitting a field entirely must not clear it.
        // Previously this was the only behavior the old serde path
        // produced — the fix preserves it while also supporting the
        // explicit-null case above.
        let mut opt = ProjectOptimization {
            start_point_x: Some(42.0),
            ..Default::default()
        };
        let patch: ProjectOptimizationPatch =
            serde_json::from_str(r#"{"inner_first": true}"#).unwrap();
        opt.apply_patch(&patch);
        assert_eq!(opt.start_point_x, Some(42.0));
    }
}
