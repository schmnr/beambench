//! Vector processing pipeline: convert, flatten, transform, boolean ops, node editing.

pub mod boolean;
pub mod buffer;
pub mod cleanup;
pub mod convert;
pub mod flatten;
pub mod node_edit;
pub mod normalize;
pub mod offset;
pub mod path_ops;
pub mod tabs;
pub mod text_to_path;
pub mod transform;
pub mod trim;

pub use boolean::{
    OFFSET_FILL_BOOLEAN_TOLERANCE_MM, apply_mask, cut_shapes,
    normalize_subject_evenodd_with_tolerance, path_intersection, path_subtract, path_union,
    weld_shapes,
};
pub use buffer::buffer_closed_path;
pub use cleanup::{dedup_consecutive_points, remove_empty_subpaths, remove_zero_length_segments};
pub use convert::{object_to_vecpath, shape_to_vecpath};
pub use flatten::flatten_vecpath;
pub use node_edit::{EditablePath, HandleType, NodeId, PathNode};
pub use normalize::{NormalizedVector, normalize_object};
pub use offset::{CornerStyle, OffsetDirection, offset_path, signed_area};
pub use path_ops::{
    FilletCandidate, apply_radius, apply_radius_at_corner, apply_start_point_edits_forward,
    auto_join_paths, break_apart, close_path, close_paths_with_tolerance, find_duplicates,
    get_fillet_candidates, optimize_path, set_start_point,
};
pub use tabs::{
    TabMarkerResolved, add_tabs, position_to_world_point, project_point_to_tab_anchor,
    resolve_tab_positions,
};
pub use transform::bake_transform;
pub use trim::{
    nearest_edge, project_point_on_polyline, project_point_on_polyline_with_dist, split_cubic,
    split_quad, trim_at_intersection, trim_at_points,
};
